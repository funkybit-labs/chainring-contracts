use arch_program::{
    account::{AccountInfo},
    entrypoint,
    program_error::ProgramError,
    pubkey::Pubkey,
    transaction_to_sign::TransactionToSign,
    program::set_transaction_to_sign,
    input_to_sign::InputToSign,
    msg,

};
use sha256::digest;
use bitcoin::{address::Address, Amount, Transaction, TxOut};
use std::str::FromStr;
use std::collections::HashMap;

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
        ProgramInstruction::BatchWithdraw(params) => withdraw_batch(program_id, accounts, &params, &params_raw_data),
        ProgramInstruction::SubmitBatchSettlement(params) => submit_settlement_batch(accounts, &params, &params_raw_data),
        ProgramInstruction::PrepareBatchSettlement(params) => prepare_settlement_batch(accounts, &params, &params_raw_data),
        ProgramInstruction::RollbackBatchSettlement() => rollback_settlement_batch(accounts),
        ProgramInstruction::RollbackBatchWithdraw(params) => rollback_withdraw_batch(accounts, &params)
    }
}

pub fn init_program_state(accounts: &[AccountInfo],
                          params: &InitProgramStateParams) -> Result<(), ProgramError> {

    let state_data: Vec<u8> = get_account_data(accounts, 0)?;
    if !state_data.is_empty() {
        return Err(ProgramError::Custom(ERROR_ALREADY_INITIALIZED));
    }
    validate_bitcoin_address(&params.program_change_address, params.network_type.clone())?;
    validate_bitcoin_address(&params.fee_account, params.network_type.clone())?;
    init_state_data(&accounts[0], ProgramState {
        version: 0,
        fee_account_address: params.fee_account.clone(),
        program_change_address: params.program_change_address.clone(),
        network_type: params.network_type.clone(),
        settlement_batch_hash: EMPTY_HASH,
        last_settlement_batch_hash: EMPTY_HASH,
        last_withdrawal_batch_hash: EMPTY_HASH,
    }.encode_to_vec().expect("Serialization error"))
}

pub fn init_token_state(accounts: &[AccountInfo],
                        params: &InitTokenStateParams) -> Result<(), ProgramError> {
    ProgramState::validate_signer(accounts)?;
    let state_data: Vec<u8> = get_account_data(accounts, 1)?;
    if !state_data.is_empty() {
        return Err(ProgramError::Custom(ERROR_ALREADY_INITIALIZED));
    }
    TokenState::initialize(
        &accounts[1],
        &params.token_id,
        &ProgramState::get_fee_account_address(&accounts[0])?,
        accounts[0].key,
    )
}

pub fn init_wallet_balances(accounts: &[AccountInfo], params: &InitWalletBalancesParams) -> Result<(), ProgramError> {
    let network_type = ProgramState::get_network_type(&accounts[0]);
    for token_state_setup in &params.token_state_setups {
        TokenState::validate_account(accounts, token_state_setup.account_index)?;
        let account = &accounts[token_state_setup.account_index as usize];
        TokenState::grow_balance_accounts_if_needed(account, token_state_setup.wallet_addresses.len())?;
        let mut num_balances = TokenState::get_num_balances(account)?;
        for wallet_address in &token_state_setup.wallet_addresses {
            validate_bitcoin_address(wallet_address, network_type.clone())?;
            Balance::set_wallet_address(account, num_balances, &wallet_address)?;
            num_balances += 1;
        }
        TokenState::set_num_balances(account, num_balances)?;
    }
    Ok(())
}


pub fn deposit_batch(accounts: &[AccountInfo],
                     params: &DepositBatchParams) -> Result<(), ProgramError> {
    if ProgramState::get_settlement_hash(&accounts[0])? != EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    for token_deposits in &params.token_deposits {
        TokenState::validate_account(accounts, token_deposits.account_index)?;
        handle_increments(&accounts[token_deposits.account_index as usize], token_deposits.clone().deposits)?;
    }
    Ok(())
}

pub fn withdraw_batch(program_id: &Pubkey, accounts: &[AccountInfo], params: &WithdrawBatchParams, params_raw_data: &[u8]) -> Result<(), ProgramError> {
    if ProgramState::get_settlement_hash(&accounts[0])? != EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    let mut tx: Transaction = bitcoin::consensus::deserialize(&params.tx_hex).unwrap();
    if tx.output.len() > 0 {
        return Err(ProgramError::Custom(ERROR_NO_OUTPUTS_ALLOWED));
    }
    let network_type = ProgramState::get_network_type(&accounts[0]);

    for token_withdrawals in &params.token_withdrawals {
        TokenState::validate_account(accounts, token_withdrawals.account_index)?;
        handle_withdrawals(
            &accounts[token_withdrawals.account_index as usize],
            token_withdrawals.clone().withdrawals,
            &ProgramState::get_fee_account_address(&accounts[0])?,
            &mut tx.output,
            network_type.clone()
        )?;
    }

    ProgramState::set_last_withdrawal_hash(&accounts[0], hash(params_raw_data))?;

    if params.change_amount > 0 {
        tx.output.push(
            TxOut {
                value: Amount::from_sat(params.change_amount),
                script_pubkey: get_bitcoin_address(
                    &ProgramState::get_program_change_address(&accounts[0])?,
                    network_type.clone()
                ).script_pubkey(),
            }
        );
    }
    msg!("withdrawal tx to send {:?}", tx);
    let mut inputs_to_sign: Vec<InputToSign> = vec![];
    for (index, _) in tx.input.iter().enumerate() {
        inputs_to_sign.push(
            InputToSign {
                index: index as u32,
                signer: program_id.clone()
            }
        )
    }
    let tx_to_sign = TransactionToSign {
        tx_bytes: &bitcoin::consensus::serialize(&tx),
        inputs_to_sign: &inputs_to_sign
    };

    set_transaction_to_sign(&[], tx_to_sign)
}

pub fn rollback_withdraw_batch(accounts: &[AccountInfo], params: &RollbackWithdrawBatchParams) -> Result<(), ProgramError> {
    for token_withdrawals in &params.token_withdrawals {
        TokenState::validate_account(accounts, token_withdrawals.account_index)?;
        handle_rollback_withdrawals(
            &accounts[token_withdrawals.account_index as usize],
            token_withdrawals.clone().withdrawals,
            &ProgramState::get_fee_account_address(&accounts[0])?,
        )?;
    }
    Ok(())
}

pub fn submit_settlement_batch(accounts: &[AccountInfo], params: &SettlementBatchParams, raw_params_data: &[u8]) -> Result<(), ProgramError> {
    let current_hash = ProgramState::get_settlement_hash(&accounts[0])?;
    let params_hash = hash(raw_params_data);

    if current_hash == EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_NO_SETTLEMENT_IN_PROGRESS));
    }
    if current_hash != params_hash {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_BATCH_MISMATCH));
    }

    for token_settlements in &params.settlements {
        TokenState::validate_account(accounts, token_settlements.account_index)?;
        let mut increments = if token_settlements.fee_amount > 0 {
            vec![Adjustment {
                address_index: AddressIndex {
                    index: FEE_ADDRESS_INDEX,
                    last4: Balance::get_wallet_address_last4(&accounts[token_settlements.account_index as usize], FEE_ADDRESS_INDEX as usize)?
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
    if ProgramState::get_settlement_hash(&accounts[0])? != EMPTY_HASH {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    let mut netting_results: HashMap<String, i64> = HashMap::new();

    for token_settlements in &params.settlements {
        TokenState::validate_account(accounts, token_settlements.account_index)?;
        let increment_sum: u64 = token_settlements.clone().increments.into_iter().map(|x| x.amount).sum::<u64>() + token_settlements.fee_amount;
        let decrement_sum: u64 = token_settlements.clone().decrements.into_iter().map(|x| x.amount).sum::<u64>();
        verify_decrements(&accounts[token_settlements.account_index as usize], token_settlements.clone().decrements)?;
        let running_netting_total = netting_results.entry(TokenState::get_token_id(&accounts[token_settlements.account_index as usize])?).or_insert(0);
        *running_netting_total += increment_sum as i64 - decrement_sum as i64;
    }

    for (token, netting_result) in &netting_results {
        if *netting_result != 0 {
            msg!("Netting for {} - value is {}", token, netting_result);
            return Err(ProgramError::Custom(ERROR_NETTING));
        }
    }

    ProgramState::set_settlement_hash(&accounts[0], hash(raw_params_data))
}

pub fn rollback_settlement_batch(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
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

fn verify_decrements(account: &AccountInfo, adjustments: Vec<Adjustment>) -> Result<(), ProgramError>{
    for adjustment in adjustments {
        let index = get_validated_index(account, &adjustment.address_index)?;
        let current_balance = Balance::get_wallet_balance(account, index)?;
        if adjustment.amount > current_balance {
            return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE));
        };
    }
    Ok(())
}
fn handle_withdrawals(
    account: &AccountInfo,
    withdrawals: Vec<Withdrawal>,
    fee_account_address: &str,
    tx_outs: &mut Vec<TxOut>,
    network_type: NetworkType,
) -> Result<(), ProgramError> {
    for withdrawal in withdrawals {
        let index = get_validated_index(account, &withdrawal.address_index)?;
        Balance::decrement_wallet_balance(account, index, withdrawal.amount)?;
        if withdrawal.fee_amount > 0 {
            if Balance::get_wallet_address(account, FEE_ADDRESS_INDEX as usize)? != fee_account_address {
                return Err(ProgramError::Custom(ERROR_ADDRESS_MISMATCH))
            }
            Balance::increment_wallet_balance(account, FEE_ADDRESS_INDEX as usize, withdrawal.fee_amount)?;
        }
        tx_outs.push(
            TxOut {
                value: Amount::from_sat(withdrawal.amount - withdrawal.fee_amount),
                script_pubkey: get_bitcoin_address(
                    &Balance::get_wallet_address(account, index)?,
                    network_type.clone()
                ).script_pubkey(),
            }
        );
    }
    Ok(())
}


fn handle_rollback_withdrawals(
    account: &AccountInfo,
    withdrawals: Vec<Withdrawal>,
    fee_account_address: &str,
) -> Result<(), ProgramError> {
    for withdrawal in withdrawals {
        let index = get_validated_index(account, &withdrawal.address_index)?;
        Balance::increment_wallet_balance(account, index, withdrawal.amount)?;
        if withdrawal.fee_amount > 0 {
            if Balance::get_wallet_address(account, FEE_ADDRESS_INDEX as usize)? != fee_account_address {
                return Err(ProgramError::Custom(ERROR_ADDRESS_MISMATCH))
            }
            Balance::decrement_wallet_balance(account, FEE_ADDRESS_INDEX as usize, withdrawal.fee_amount)?;
        }
    }
    Ok(())
}


//
// Helper methods
//

fn validate_bitcoin_address(address: &str, network_type: NetworkType) -> Result<(), ProgramError> {
    match Address::from_str(address).unwrap().require_network(map_network_type(network_type)) {
        Ok(_) => Ok(()),
        Err(_) => Err(ProgramError::Custom(ERROR_INVALID_ADDRESS)),
    }
}

fn get_bitcoin_address(address: &str, network_type: NetworkType) -> Address {
    Address::from_str(address).unwrap().require_network(map_network_type(network_type)).unwrap()
}

fn map_network_type(network_type: NetworkType) -> bitcoin::Network {
    match network_type {
        NetworkType::Bitcoin => bitcoin::Network::Bitcoin,
        NetworkType::Testnet => bitcoin::Network::Testnet,
        NetworkType::Signet => bitcoin::Network::Signet,
        NetworkType::Regtest => bitcoin::Network::Regtest
    }
}

fn init_state_data(account: &AccountInfo, new_data: Vec<u8>) -> Result<(), ProgramError> {
    if new_data.len() > entrypoint::MAX_PERMITTED_DATA_LENGTH as usize {
        return Err(ProgramError::InvalidRealloc);
    }
    account.realloc(new_data.len(), true)?;
    account.data.try_borrow_mut().unwrap().copy_from_slice(new_data.as_slice());
    Ok(())
}

fn get_account_data(accounts: &[AccountInfo], index: u8) -> Result<Vec<u8>, ProgramError> {
    if index as usize >= accounts.len() {
        return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_INDEX));
    }
    Ok(accounts[index as usize].data.try_borrow().unwrap().to_vec())
}

pub fn get_validated_index(account: &AccountInfo, address_index: &AddressIndex) -> Result<usize, ProgramError> {
    let index = address_index.index as usize;
    if index >= TokenState::get_num_balances(account)? {
        return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
    }
    let wallet_address = Balance::get_wallet_address(account, index)?;
    if wallet_last4(&wallet_address) != address_index.last4 {
        return Err(ProgramError::Custom(ERROR_WALLET_LAST4_MISMATCH))
    }
    Ok(index)
}
