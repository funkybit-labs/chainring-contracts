use arch_program::{
    account::{AccountInfo},
    entrypoint,
    program_error::ProgramError,
    pubkey::Pubkey,
    transaction_to_sign::TransactionToSign,
    program::{get_account_script_pubkey, set_transaction_to_sign},
    input_to_sign::InputToSign,
    helper::get_state_transition_tx,
    msg,
};
use sha256::digest;
use bitcoin::{Amount, ScriptBuf, Transaction, TxOut};
use std::collections::{HashMap, HashSet};
use ordinals::{Edict, RuneId, Runestone};

use model::state::*;
use model::instructions::*;
use model::error::*;
use model::serialization::Codable;

entrypoint!(process_instruction);
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {
    let instruction = ProgramInstruction::decode_from_slice(instruction_data)
        .map_err(|_| ProgramError::InvalidInstructionData)?;
    let params_raw_data = ProgramInstruction::params_raw_data(&instruction_data);

    match instruction {
        ProgramInstruction::InitProgramState(params) => init_program_state(accounts, &params),
        ProgramInstruction::InitTokenState(params) => init_token_state(accounts, &params),
        ProgramInstruction::InitWalletBalances(params) => init_wallet_balances(accounts, &params),
        ProgramInstruction::BatchDeposit(params) => deposit_batch(accounts, &params),
        ProgramInstruction::PrepareBatchWithdraw(params) => prepare_withdraw_batch(accounts, &params, &params_raw_data),
        ProgramInstruction::SubmitBatchSettlement(params) => submit_settlement_batch(accounts, &params, &params_raw_data),
        ProgramInstruction::PrepareBatchSettlement(params) => prepare_settlement_batch(accounts, &params, &params_raw_data),
        ProgramInstruction::RollbackBatchSettlement() => rollback_settlement_batch(accounts),
        ProgramInstruction::RollbackBatchWithdraw(params) => rollback_withdraw_batch(accounts, &params),
        ProgramInstruction::SubmitBatchWithdraw(params) => submit_withdraw_batch(program_id, accounts, &params, &params_raw_data),
        ProgramInstruction::InitRuneReceiverState() => init_rune_receiver_state(accounts),
    }
}

pub fn init_program_state(accounts: &[AccountInfo],
                          params: &InitProgramStateParams) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, true, None, None)?;
    validate_account(accounts, 1, false, true, None, None)?;
    if accounts.len() == 3 {
        validate_account(accounts, 2, false, true, None, None)?;
    }
    validate_bitcoin_address(&params.program_change_address, params.network_type.clone(), true)?;
    validate_bitcoin_address(&params.fee_account, params.network_type.clone(), true)?;
    init_state_data(&accounts[0], ProgramState {
        account_type: AccountType::Program,
        version: 0,
        withdraw_account: *accounts[1].key,
        fee_account_address: params.fee_account.clone(),
        program_change_address: params.program_change_address.clone(),
        network_type: params.network_type.clone(),
        settlement_batch_hash: EMPTY_HASH,
        last_settlement_batch_hash: EMPTY_HASH,
        events: vec![],
    }.encode_to_vec().expect("Serialization error"), EVENT_SIZE * MAX_EVENTS)?;
    if accounts.len() == 3 {
        RuneReceiverState::initialize(accounts, 2)?;
    }
    WithdrawState::initialize(&accounts)
}

pub fn init_token_state(accounts: &[AccountInfo],
                        params: &InitTokenStateParams) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, false, Some(AccountType::Program), None)?;
    validate_account(accounts, 1, false, true, None, None)?;
    TokenState::initialize(
        &accounts[1],
        &params.token_id,
        &ProgramState::get_fee_account_address(&accounts[0])?,
        accounts[0].key,
    )
}

pub fn init_rune_receiver_state(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, true, Some(AccountType::Program), None)?;
    validate_account(accounts, 1, false, true, None, None)?;
    RuneReceiverState::initialize(accounts, 1)
}


pub fn init_wallet_balances(accounts: &[AccountInfo], params: &InitWalletBalancesParams) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, false, Some(AccountType::Program), None)?;
    let network_type = ProgramState::get_network_type(&accounts[0]);
    for token_state_setup in &params.token_state_setups {
        validate_account(accounts, token_state_setup.account_index, false, true, Some(AccountType::Token), Some(0))?;
        let account = &accounts[token_state_setup.account_index as usize];
        TokenState::grow_balance_accounts_if_needed(account, token_state_setup.wallet_addresses.len())?;
        let mut num_balances = TokenState::get_num_balances(account)?;
        for wallet_address in &token_state_setup.wallet_addresses {
            validate_bitcoin_address(wallet_address, network_type.clone(), false)?;
            Balance::set_wallet_address(account, num_balances, &wallet_address)?;
            num_balances += 1;
        }
        TokenState::set_num_balances(account, num_balances)?;
    }
    Ok(())
}


pub fn deposit_batch(accounts: &[AccountInfo],
                     params: &DepositBatchParams) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, false, Some(AccountType::Program), None)?;
    for token_deposits in &params.token_deposits {
        validate_account(accounts, token_deposits.account_index, false, true, Some(AccountType::Token), Some(0))?;
        handle_increments(&accounts[token_deposits.account_index as usize], token_deposits.clone().deposits)?;
    }
    Ok(())
}

pub fn prepare_withdraw_batch(accounts: &[AccountInfo], params: &WithdrawBatchParams, params_raw_data: &[u8]) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, true, Some(AccountType::Program), Some(1))?;
    validate_account(accounts, 1, false, true, Some(AccountType::Withdraw), Some(0))?;
    let has_rune_receiver = if get_type(&accounts[2])? == AccountType::RuneReceiver {
        validate_account(accounts, 2, false, false, Some(AccountType::RuneReceiver), Some(0))?;
        true
    } else {
        false
    };
    if ProgramState::get_settlement_hash(&accounts[0])? != EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    if WithdrawState::get_hash(&accounts[1])? != EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_WITHDRAWAL_IN_PROGRESS));
    }

    let mut tx: Transaction = bitcoin::consensus::deserialize(&params.tx_hex)
        .map_err(|_| ProgramError::Custom(ERROR_INVALID_INPUT_TX))?;
    if tx.output.len() > 0 {
        return Err(ProgramError::Custom(ERROR_NO_OUTPUTS_ALLOWED));
    }
    if tx.input.len() != params.input_utxo_types.len() {
        return Err(ProgramError::Custom(ERROR_INVALID_UTXO_TYPES));
    }

    ProgramState::clear_events(&accounts[0])?;
    let network_type = ProgramState::get_network_type(&accounts[0]);

    for token_withdrawals in &params.token_withdrawals {
        validate_account(accounts, token_withdrawals.account_index, false, true, Some(AccountType::Token), Some(0))?;
        validate_account(accounts, token_withdrawals.fee_account_index, false, true, Some(AccountType::Token), Some(0))?;
        verify_withdrawals(
            &accounts,
            token_withdrawals.account_index,
            token_withdrawals.fee_account_index,
            token_withdrawals.clone().withdrawals,
            network_type.clone(),
        )?;
    }

    if ProgramState::get_events_count(&accounts[0])? != 0 {
        return Ok(());
    }

    let mut edicts: Vec<Edict> = vec![];

    // Apply all the changes in the batch
    for token_withdrawals in &params.token_withdrawals {
        handle_prepare_withdrawals(
            &accounts[token_withdrawals.account_index as usize],
            &accounts[token_withdrawals.fee_account_index as usize],
            token_withdrawals.clone().withdrawals,
            &ProgramState::get_fee_account_address(&accounts[0])?,
            &mut tx.output,
            network_type.clone(),
            &mut edicts,
        )?;
    }

    if !edicts.is_empty() && !has_rune_receiver {
        return Err(ProgramError::Custom(ERROR_NO_RUNE_RECEIVER));
    }

    if tx.output.len() == 0 {
        return Err(ProgramError::Custom(ERROR_NO_TX_OUTPUTS));
    }
    WithdrawState::set_hash(&accounts[1], hash(params_raw_data))
}

pub fn submit_withdraw_batch(program_id: &Pubkey, accounts: &[AccountInfo], params: &WithdrawBatchParams, params_raw_data: &[u8]) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, false, Some(AccountType::Program), Some(1))?;
    validate_account(accounts, 1, true, true, Some(AccountType::Withdraw), Some(0))?;
    let has_rune_receiver = if get_type(&accounts[2])? == AccountType::RuneReceiver {
        validate_account(accounts, 2, true, true, Some(AccountType::RuneReceiver), Some(0))?;
        true
    } else {
        false
    };

    if WithdrawState::get_hash(&accounts[1])? != hash(params_raw_data) {
        return Err(ProgramError::Custom(ERROR_WITHDRAWAL_BATCH_MISMATCH));
    }
    let network_type = ProgramState::get_network_type(&accounts[0]);

    let mut tx = get_state_transition_tx(accounts);

    let num_state_transitions = tx.output.len();

    let tx_with_inputs: Transaction = bitcoin::consensus::deserialize(&params.tx_hex)
        .map_err(|_| ProgramError::Custom(ERROR_INVALID_INPUT_TX))?;
    for input in tx_with_inputs.input.iter() {
        tx.input.push(input.clone())
    }

    let mut edicts: Vec<Edict> = vec![];

    for token_withdrawals in &params.token_withdrawals {
        validate_account(accounts, token_withdrawals.account_index, false, false, Some(AccountType::Token), Some(0))?;
        validate_account(accounts, token_withdrawals.fee_account_index, false, false, Some(AccountType::Token), Some(0))?;
        handle_submit_withdrawals(
            &accounts[token_withdrawals.account_index as usize],
            token_withdrawals.clone().withdrawals,
            &mut tx.output,
            network_type.clone(),
            &mut edicts,
        )?;
    }

    if !edicts.is_empty() && !has_rune_receiver {
        return Err(ProgramError::Custom(ERROR_NO_RUNE_RECEIVER));
    }

    if params.change_amount > 0 {
        tx.output.push(
            TxOut {
                value: Amount::from_sat(params.change_amount),
                script_pubkey: ScriptBuf::from_bytes(get_account_script_pubkey(program_id).to_vec()),
            }
        );
    }

    if !edicts.is_empty() {
        let rune_ids: HashSet<RuneId> = HashSet::from_iter(edicts.iter().map(|e| e.id).collect::<Vec<RuneId>>().to_vec());
        rune_ids.into_iter().for_each(|rune_id| {
            add_edict_and_ouput(
                rune_id,
                &mut tx.output,
                &mut edicts,
                ScriptBuf::from_bytes(get_account_script_pubkey(accounts[2].key).to_vec()),
                0,
            ).expect("cannot add change edict");
        });

        let runestone = Runestone {
            edicts,
            etching: None,
            mint: None,
            pointer: None,
        };

        let runestone_bytes = runestone.encipher().to_bytes();
        tx.output.push(
            TxOut {
                script_pubkey: ScriptBuf::from_bytes(runestone_bytes.clone()),
                value: Amount::from_sat(0),
            },
        );
    }


    let mut inputs_to_sign: Vec<InputToSign> = vec![];
    for (index, _) in tx.input.iter().enumerate() {
        inputs_to_sign.push(
            InputToSign {
                index: index as u32,
                signer: if index == 0 {
                    *accounts[1].key
                } else if has_rune_receiver && index == 1 {
                    *accounts[2].key
                } else {
                    if params.input_utxo_types[index - num_state_transitions] == InputUtxoType::Bitcoin {
                        program_id.clone()
                    } else {
                        *accounts[2].key
                    }
                },
            }
        )
    }


    let tx_to_sign = TransactionToSign {
        tx_bytes: &bitcoin::consensus::serialize(&tx),
        inputs_to_sign: &inputs_to_sign,
    };

    set_transaction_to_sign(accounts, tx_to_sign)?;

    WithdrawState::clear_hash(&accounts[1])
}

pub fn rollback_withdraw_batch(accounts: &[AccountInfo], params: &RollbackWithdrawBatchParams) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, false, Some(AccountType::Program), Some(1))?;
    validate_account(accounts, 1, false, true, Some(AccountType::Withdraw), Some(0))?;
    for token_withdrawals in &params.token_withdrawals {
        validate_account(accounts, token_withdrawals.account_index, false, true, Some(AccountType::Token), Some(0))?;
        validate_account(accounts, token_withdrawals.fee_account_index, false, true, Some(AccountType::Token), Some(0))?;
        handle_rollback_withdrawals(
            &accounts[token_withdrawals.account_index as usize],
            &accounts[token_withdrawals.fee_account_index as usize],
            token_withdrawals.clone().withdrawals,
            &ProgramState::get_fee_account_address(&accounts[0])?,
        )?;
    }
    WithdrawState::clear_hash(&accounts[1])?;
    Ok(())
}

pub fn submit_settlement_batch(accounts: &[AccountInfo], params: &SettlementBatchParams, raw_params_data: &[u8]) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, true, Some(AccountType::Program), None)?;
    let current_hash = ProgramState::get_settlement_hash(&accounts[0])?;
    let params_hash = hash(raw_params_data);

    if current_hash == EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_NO_SETTLEMENT_IN_PROGRESS));
    }
    if current_hash != params_hash {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_BATCH_MISMATCH));
    }

    for token_settlements in &params.settlements {
        validate_account(accounts, token_settlements.account_index, false, true, Some(AccountType::Token), Some(0))?;
        let mut increments = if token_settlements.fee_amount > 0 {
            vec![Adjustment {
                address_index: AddressIndex {
                    index: FEE_ADDRESS_INDEX,
                    last4: Balance::get_wallet_address_last4(&accounts[token_settlements.account_index as usize], FEE_ADDRESS_INDEX as usize)?,
                },
                amount: token_settlements.fee_amount,
            }]
        } else {
            vec![]
        };
        increments.append(&mut token_settlements.clone().increments);
        handle_increments(&accounts[token_settlements.account_index as usize], increments)?;
        handle_decrements(&accounts[token_settlements.account_index as usize], token_settlements.clone().decrements)?;
    }

    ProgramState::set_last_settlement_hash(&accounts[0], params_hash)?;
    ProgramState::clear_settlement_hash(&accounts[0])
}

pub fn prepare_settlement_batch(accounts: &[AccountInfo], params: &SettlementBatchParams, raw_params_data: &[u8]) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, true, Some(AccountType::Program), None)?;
    if ProgramState::get_settlement_hash(&accounts[0])? != EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    ProgramState::clear_events(&accounts[0])?;
    let mut netting_results: HashMap<String, i64> = HashMap::new();

    for token_settlements in &params.settlements {
        validate_account(accounts, token_settlements.account_index, false, false, Some(AccountType::Token), Some(0))?;
        let increment_sum: u64 = token_settlements.clone().increments.into_iter().map(|x| x.amount).sum::<u64>() + token_settlements.fee_amount;
        let decrement_sum: u64 = token_settlements.clone().decrements.into_iter().map(|x| x.amount).sum::<u64>();
        verify_decrements(&accounts, token_settlements.account_index, token_settlements.clone().decrements)?;
        verify_increments(&accounts, token_settlements.account_index, token_settlements.clone().increments)?;
        let running_netting_total = netting_results.entry(TokenState::get_token_id(&accounts[token_settlements.account_index as usize])?).or_insert(0);
        *running_netting_total += increment_sum as i64 - decrement_sum as i64;
    }

    for (token, netting_result) in &netting_results {
        if *netting_result != 0 {
            msg!("Netting for {} - value is {}", token, netting_result);
            return Err(ProgramError::Custom(ERROR_NETTING));
        }
    }

    if ProgramState::get_events_count(&accounts[0])? == 0 {
        ProgramState::set_settlement_hash(&accounts[0], hash(raw_params_data))
    } else {
        Ok(())
    }
}

pub fn rollback_settlement_batch(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
    validate_account(accounts, 0, true, true, Some(AccountType::Program), None)?;
    ProgramState::clear_settlement_hash(&accounts[0])
}

fn hash(data: &[u8]) -> Hash {
    let mut tmp = EMPTY_HASH;
    tmp[..32].copy_from_slice(&hex::decode(digest(data)).unwrap());
    tmp
}

fn handle_increments(account: &AccountInfo, adjustments: Vec<Adjustment>) -> Result<(), ProgramError> {
    handle_adjustments(account, adjustments, true)
}

fn handle_decrements(account: &AccountInfo, adjustments: Vec<Adjustment>) -> Result<(), ProgramError> {
    handle_adjustments(account, adjustments, false)
}

fn handle_adjustments(account: &AccountInfo, adjustments: Vec<Adjustment>, increment: bool) -> Result<(), ProgramError> {
    for adjustment in adjustments {
        let index = get_validated_index(account, &adjustment.address_index)?;
        if increment {
            Balance::increment_wallet_balance(account, index, adjustment.amount)?;
        } else {
            Balance::decrement_wallet_balance(account, index, adjustment.amount)?;
        }
    }
    Ok(())
}

fn verify_decrements(accounts: &[AccountInfo], account_index: u8, adjustments: Vec<Adjustment>) -> Result<(), ProgramError> {
    let account = &accounts[account_index as usize];
    for adjustment in adjustments {
        let index = get_validated_index(account, &adjustment.address_index)?;
        let current_balance = Balance::get_wallet_balance(account, index)?;
        if adjustment.amount > current_balance {
            ProgramState::emit_event(
                &accounts[0],
                &Event::FailedSettlement {
                    account_index,
                    address_index: adjustment.address_index.index,
                    requested_amount: adjustment.amount,
                    balance: current_balance,
                    error_code: ERROR_INSUFFICIENT_BALANCE,
                })?;
        };
    }
    Ok(())
}

fn verify_increments(accounts: &[AccountInfo], account_index: u8, adjustments: Vec<Adjustment>) -> Result<(), ProgramError> {
    let account = &accounts[account_index as usize];
    for adjustment in adjustments {
        let _ = get_validated_index(account, &adjustment.address_index)?;
    }
    Ok(())
}

fn verify_withdrawals(accounts: &[AccountInfo], account_index: u8, fee_account_index: u8, withdrawals: Vec<Withdrawal>, network_type: NetworkType) -> Result<(), ProgramError> {
    let account = &accounts[account_index as usize];
    let fee_account = &accounts[fee_account_index as usize];
    for withdrawal in withdrawals {
        let index_result = get_validated_index_withdraw(account, &withdrawal.address_index, network_type.clone());
        match index_result {
            Ok(index) => {
                let current_balance = Balance::get_wallet_balance(account, index)?;
                let fee_index = get_validated_index_withdraw(fee_account, &withdrawal.fee_address_index, network_type.clone())?;
                let balance_in_fee_token = Balance::get_wallet_balance(fee_account, fee_index)?;
                if withdrawal.amount > current_balance || withdrawal.fee_amount > balance_in_fee_token {
                    ProgramState::emit_event(
                        &accounts[0],
                        &Event::FailedWithdrawal {
                            account_index,
                            address_index: withdrawal.address_index.index,
                            fee_account_index,
                            fee_address_index: withdrawal.fee_address_index.index,
                            requested_amount: withdrawal.amount,
                            fee_amount: withdrawal.fee_amount,
                            balance: current_balance,
                            balance_in_fee_token,
                            error_code: ERROR_INSUFFICIENT_BALANCE,
                        },
                    )?;
                };
            }
            Err(_) => {
                ProgramState::emit_event(
                    &accounts[0],
                    &Event::FailedWithdrawal {
                        account_index,
                        address_index: withdrawal.address_index.index,
                        fee_account_index,
                        fee_address_index: withdrawal.fee_address_index.index,
                        requested_amount: withdrawal.amount,
                        fee_amount: withdrawal.fee_amount,
                        balance: 0,
                        balance_in_fee_token: 0,
                        error_code: ERROR_INVALID_ADDRESS_NETWORK,
                    },
                )?;
            }
        }
    }
    Ok(())
}

fn handle_prepare_withdrawals(
    account: &AccountInfo,
    fee_account: &AccountInfo,
    withdrawals: Vec<Withdrawal>,
    fee_account_address: &str,
    tx_outs: &mut Vec<TxOut>,
    network_type: NetworkType,
    edicts: &mut Vec<Edict>,
) -> Result<(), ProgramError> {
    for withdrawal in withdrawals {
        Balance::decrement_wallet_balance(account, withdrawal.address_index.index as usize, withdrawal.amount)?;
        if withdrawal.fee_amount > 0 {
            if Balance::get_wallet_address(fee_account, FEE_ADDRESS_INDEX as usize)? != fee_account_address {
                return Err(ProgramError::Custom(ERROR_ADDRESS_MISMATCH));
            }
            Balance::increment_wallet_balance(fee_account, FEE_ADDRESS_INDEX as usize, withdrawal.fee_amount)?;
            if fee_account.key != account.key {
                Balance::decrement_wallet_balance(fee_account, withdrawal.fee_address_index.index as usize, withdrawal.fee_amount)?;
            }
        }
        add_withdrawal_output(account, &withdrawal, tx_outs, network_type.clone(), edicts)?;
    }
    Ok(())
}

fn handle_submit_withdrawals(
    account: &AccountInfo,
    withdrawals: Vec<Withdrawal>,
    tx_outs: &mut Vec<TxOut>,
    network_type: NetworkType,
    edicts: &mut Vec<Edict>,
) -> Result<(), ProgramError> {
    for withdrawal in withdrawals {
        let _ = get_validated_index(account, &withdrawal.address_index)?;
        add_withdrawal_output(account, &withdrawal, tx_outs, network_type.clone(), edicts)?;
    }
    Ok(())
}

fn add_withdrawal_output(
    account: &AccountInfo,
    withdrawal: &Withdrawal,
    tx_outs: &mut Vec<TxOut>,
    network_type: NetworkType,
    edicts: &mut Vec<Edict>,
) -> Result<(), ProgramError> {
    let is_rune = TokenState::is_rune_account(account);
    if is_rune {
        add_edict_and_ouput(
            TokenState::get_rune_id(account)?,
            tx_outs,
            edicts,
            get_bitcoin_address(
                &Balance::get_wallet_address(account, withdrawal.address_index.index as usize)?,
                network_type.clone(),
            ).script_pubkey(),
            withdrawal.amount,
        )?;
    } else {
        tx_outs.push(
            TxOut {
                value: Amount::from_sat(withdrawal.amount - withdrawal.fee_amount),
                script_pubkey: get_bitcoin_address(
                    &Balance::get_wallet_address(account, withdrawal.address_index.index as usize)?,
                    network_type.clone(),
                ).script_pubkey(),
            }
        );
    }
    Ok(())
}

fn add_edict_and_ouput(
    rune_id: RuneId,
    tx_outs: &mut Vec<TxOut>,
    edicts: &mut Vec<Edict>,
    script_buf: ScriptBuf,
    edict_amount: u64,
) -> Result<(), ProgramError> {
    edicts.push(Edict {
        id: rune_id,
        amount: edict_amount as u128,
        output: tx_outs.len() as u32,
    });
    tx_outs.push(
        TxOut {
            value: Amount::from_sat(547),
            script_pubkey: script_buf,
        }
    );
    Ok(())
}

fn handle_rollback_withdrawals(
    account: &AccountInfo,
    fee_account: &AccountInfo,
    withdrawals: Vec<Withdrawal>,
    fee_account_address: &str,
) -> Result<(), ProgramError> {
    for withdrawal in withdrawals {
        let index = get_validated_index(account, &withdrawal.address_index)?;
        Balance::increment_wallet_balance(account, index, withdrawal.amount)?;
        if withdrawal.fee_amount > 0 {
            if Balance::get_wallet_address(fee_account, FEE_ADDRESS_INDEX as usize)? != fee_account_address {
                return Err(ProgramError::Custom(ERROR_ADDRESS_MISMATCH));
            }
            Balance::decrement_wallet_balance(fee_account, FEE_ADDRESS_INDEX as usize, withdrawal.fee_amount)?;
            if fee_account.key != account.key {
                Balance::increment_wallet_balance(fee_account, withdrawal.fee_address_index.index as usize, withdrawal.fee_amount)?;
            }
        }
    }
    Ok(())
}


//
// Helper methods
//

fn init_state_data(account: &AccountInfo, new_data: Vec<u8>, additional_bytes: usize) -> Result<(), ProgramError> {
    if new_data.len() + additional_bytes > entrypoint::MAX_PERMITTED_DATA_LENGTH as usize {
        return Err(ProgramError::InvalidRealloc);
    }
    account.realloc(new_data.len() + additional_bytes, true)?;
    account.data.try_borrow_mut().unwrap()[0..EVENTS_OFFSET].copy_from_slice(new_data.as_slice());
    Ok(())
}

pub fn get_validated_index(account: &AccountInfo, address_index: &AddressIndex) -> Result<usize, ProgramError> {
    let index = address_index.index as usize;
    if index >= TokenState::get_num_balances(account)? {
        return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX));
    }
    let wallet_address = Balance::get_wallet_address(account, index)?;
    if wallet_last4(&wallet_address) != address_index.last4 {
        return Err(ProgramError::Custom(ERROR_WALLET_LAST4_MISMATCH));
    }
    Ok(index)
}

pub fn get_validated_index_withdraw(account: &AccountInfo, address_index: &AddressIndex, network_type: NetworkType) -> Result<usize, ProgramError> {
    let index = get_validated_index(account, address_index)?;
    let wallet_address = Balance::get_wallet_address(account, index)?;
    validate_bitcoin_address(&wallet_address, network_type, true)?;
    Ok(index)
}
