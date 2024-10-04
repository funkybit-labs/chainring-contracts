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
use borsh::{BorshDeserialize, BorshSerialize};
use sha256::digest;
use bitcoin::{address::Address, Amount, Transaction, TxOut};
use std::str::FromStr;
use std::collections::HashMap;
use shared::create_bitcoin_withdrawal_message;
use shared::sig::BitcoinSignatureVerification;
use shared::address::BitcoinAddress;

const ERROR_INVALID_ADDRESS_INDEX: u32 = 601;
const ERROR_INVALID_ACCOUNT_INDEX: u32 = 602;
const ERROR_INSUFFICIENT_BALANCE: u32 = 603;
const ERROR_ADDRESS_MISMATCH: u32 = 604;
const ERROR_SETTLEMENT_IN_PROGRESS: u32 = 605;
const ERROR_NO_SETTLEMENT_IN_PROGRESS: u32 = 606;
const ERROR_SETTLEMENT_BATCH_MISMATCH: u32 = 607;
const ERROR_NETTING: u32 = 608;
const ERROR_ALREADY_INITIALIZED: u32 = 609;
const ERROR_PROGRAM_STATE_MISMATCH: u32 = 610;
const ERROR_NO_OUTPUTS_ALLOWED: u32 = 611;
const ERROR_INVALID_ADDRESS: u32 = 612;
const ERROR_SIGNATURE_NOT_VALID: u32 = 613;


entrypoint!(process_instruction);
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {

    let exchange_instruction: ProgramInstruction = borsh::from_slice(instruction_data).unwrap();
    match exchange_instruction {
        ProgramInstruction::InitProgramState(params) =>  init_program_state(accounts, params),
        ProgramInstruction::InitTokenState(params) =>  init_token_state(accounts, params),
        ProgramInstruction::InitWalletBalances(params) =>  init_wallet_balances(accounts, params),
        ProgramInstruction::BatchDeposit(params) => deposit_batch(accounts, params),
        ProgramInstruction::BatchWithdraw(params) => withdraw_batch(program_id, accounts, params),
        ProgramInstruction::SubmitBatchSettlement(params) => submit_settlement_batch(accounts, params),
        ProgramInstruction::PrepareBatchSettlement(params) => prepare_settlement_batch(accounts, params),
        ProgramInstruction::RollbackBatchSettlement() => rollback_settlement_batch(accounts)
    }
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub enum NetworkType {
    /// Mainnet Bitcoin.
    Bitcoin,
    /// Bitcoin's testnet network.
    Testnet,
    /// Bitcoin's signet network.
    Signet,
    /// Bitcoin's regtest network.
    Regtest,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Balance {
    pub address: String,
    pub balance: u64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct TokenBalances {
    pub version: u16,
    pub program_state_account: Pubkey,
    pub token_id: String,
    pub balances: Vec<Balance>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct ProgramState {
    pub version: u16,
    pub fee_account_address: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
    pub settlement_batch_hash: String,
    pub last_settlement_batch_hash: String,
    pub last_withdrawal_batch_hash: String,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Adjustment {
    pub address_index: u32,
    pub amount: u64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct TokenBalanceSetup {
    pub account_index: u8,
    pub wallet_addresses: Vec<String>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Withdrawal {
    pub address_index: u32,
    pub amount: u64,
    pub fee_amount: u64,
    pub signature: Vec<u8>,
    pub timestamp: i64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub enum ProgramInstruction {
    InitProgramState(InitProgramStateParams),
    InitTokenState(InitTokenStateParams),
    InitWalletBalances(InitWalletBalancesParams),
    BatchDeposit(DepositBatchParams),
    BatchWithdraw(WithdrawBatchParams),
    PrepareBatchSettlement(SettlementBatchParams),
    SubmitBatchSettlement(SettlementBatchParams),
    RollbackBatchSettlement(),
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct InitProgramStateParams {
    pub fee_account: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct InitTokenStateParams {
    pub token_id: String,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct InitWalletBalancesParams {
    pub token_balance_setups: Vec<TokenBalanceSetup>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct DepositBatchParams {
    pub token_deposits: Vec<TokenDeposits>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct TokenDeposits {
    pub account_index: u8,
    pub deposits: Vec<Adjustment>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct TokenWithdrawals {
    pub account_index: u8,
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct WithdrawBatchParams {
    pub token_withdrawals: Vec<TokenWithdrawals>,
    pub change_amount: u64,
    pub tx_hex: Vec<u8>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct SettlementAdjustments {
    pub account_index: u8,
    pub increments: Vec<Adjustment>,
    pub decrements: Vec<Adjustment>,
    pub fee_amount: u64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct SettlementBatchParams {
    pub settlements: Vec<SettlementAdjustments>
}

const FEE_ADDRESS_INDEX: u32 = 0;

pub fn init_program_state(accounts: &[AccountInfo],
                          params: InitProgramStateParams) -> Result<(), ProgramError> {

    let state_data: Vec<u8> = get_account_data(accounts, 0)?;
    if !state_data.is_empty() {
        return Err(ProgramError::Custom(ERROR_ALREADY_INITIALIZED));
    }
    validate_bitcoin_address(&params.program_change_address, params.network_type.clone())?;
    let state = ProgramState {
        version: 0,
        fee_account_address: params.fee_account,
        program_change_address: params.program_change_address,
        network_type: params.network_type,
        settlement_batch_hash: "".to_string(),
        last_settlement_batch_hash: "".to_string(),
        last_withdrawal_batch_hash: "".to_string(),
    };
    update_state_data(&accounts[0], borsh::to_vec(&state).unwrap())
}

pub fn init_token_state(accounts: &[AccountInfo],
                        params: InitTokenStateParams) -> Result<(), ProgramError> {
    let state_data: Vec<u8> = get_account_data(accounts, 1)?;
    if !state_data.is_empty() {
        return Err(ProgramError::Custom(ERROR_ALREADY_INITIALIZED));
    }
    let program_state: ProgramState = get_account_state(accounts, 0)?;
    update_state_data(&accounts[1], borsh::to_vec(&TokenBalances {
        version: 0,
        program_state_account: *accounts[0].key,
        token_id: params.token_id,
        balances: vec![Balance{ address: program_state.fee_account_address, balance: 0 }],
    }).unwrap())
}

pub fn init_wallet_balances(accounts: &[AccountInfo], params: InitWalletBalancesParams) -> Result<(), ProgramError> {
    let program_state: ProgramState = get_account_state(accounts, 0)?;
    for token_balance_setup in &params.token_balance_setups {
        let mut token_balance_state = get_token_balance_state(accounts, 1)?;
        for wallet_address in &token_balance_setup.wallet_addresses {
            validate_bitcoin_address(wallet_address, program_state.network_type.clone())?;
            token_balance_state.balances.push(Balance {
                address: wallet_address.to_string(),
                balance: 0,
            })
        }
        update_state_data(
            &accounts[token_balance_setup.account_index as usize],
            borsh::to_vec(&token_balance_state).unwrap()
        )?;
    }
    Ok(())
}


pub fn deposit_batch(accounts: &[AccountInfo],
                     params: DepositBatchParams) -> Result<(), ProgramError> {
    let program_state: ProgramState = get_account_state(accounts, 0)?;

    if !program_state.settlement_batch_hash.is_empty() {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    for token_deposits in &params.token_deposits {
        let token_balance_state = get_token_balance_state(accounts, token_deposits.account_index)?;
        let updated_token_balance_state = handle_increments(token_balance_state, token_deposits.clone().deposits)?;
        update_state_data(
            &accounts[token_deposits.account_index as usize],
            borsh::to_vec(&updated_token_balance_state).unwrap()
        )?;
    }
    Ok(())
}

pub fn withdraw_batch(program_id: &Pubkey, accounts: &[AccountInfo], params: WithdrawBatchParams) -> Result<(), ProgramError> {
    let mut program_state: ProgramState = get_account_state(accounts, 0)?;

    if !program_state.settlement_batch_hash.is_empty() {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    let mut tx: Transaction = bitcoin::consensus::deserialize(&params.tx_hex).unwrap();
    if tx.output.len() > 0 {
        return Err(ProgramError::Custom(ERROR_NO_OUTPUTS_ALLOWED));
    }

    for token_withdrawals in &params.token_withdrawals {
        let token_balance_state = get_token_balance_state(accounts, token_withdrawals.account_index)?;
        let updated_token_balance_state = handle_withdrawals(
            token_balance_state,
            token_withdrawals.clone().withdrawals,
            &program_state.fee_account_address,
            &mut tx.output,
            program_state.network_type.clone()
        )?;
        update_state_data(
            &accounts[token_withdrawals.account_index as usize],
            borsh::to_vec(&updated_token_balance_state).unwrap()
        )?;
    }

    program_state.last_withdrawal_batch_hash = hash(borsh::to_vec(&params).unwrap());
    update_state_data(
        &accounts[0],
        borsh::to_vec(&program_state).unwrap()
    )?;

    if params.change_amount > 0 {
        tx.output.push(
            TxOut {
                value: Amount::from_sat(params.change_amount),
                script_pubkey: get_bitcoin_address(
                    &program_state.program_change_address,
                    program_state.network_type.clone()
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

pub fn submit_settlement_batch(accounts: &[AccountInfo], params: SettlementBatchParams) -> Result<(), ProgramError> {
    let mut program_state: ProgramState = get_account_state(accounts, 0)?;

    if program_state.settlement_batch_hash.is_empty() {
        return Err(ProgramError::Custom(ERROR_NO_SETTLEMENT_IN_PROGRESS));
    }
    if program_state.settlement_batch_hash != hash(borsh::to_vec(&params).unwrap()) {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_BATCH_MISMATCH));
    }

    for token_settlements in &params.settlements {
        let token_balance_state = get_token_balance_state(accounts, token_settlements.account_index)?;
        let mut increments = if token_settlements.fee_amount > 0 {
            vec![Adjustment {
                address_index: FEE_ADDRESS_INDEX,
                amount: token_settlements.fee_amount,
            }]
        } else {
            vec![]
        };
        increments.append(&mut token_settlements.clone().increments);
        let token_balance_state_1 = handle_increments(token_balance_state, increments)?;
        let token_balance_state_2 = handle_decrements(token_balance_state_1, token_settlements.clone().decrements)?;
        update_state_data(
            &accounts[token_settlements.account_index as usize],
            borsh::to_vec(&token_balance_state_2).unwrap()
        )?;
    }

    program_state.last_settlement_batch_hash = hash(borsh::to_vec(&params).unwrap());
    program_state.settlement_batch_hash = "".to_string();
    update_state_data(
        &accounts[0],
        borsh::to_vec(&program_state).unwrap()
    )
}

pub fn prepare_settlement_batch(accounts: &[AccountInfo], params: SettlementBatchParams) -> Result<(), ProgramError> {
    let mut program_state: ProgramState = get_account_state(accounts, 0)?;

    if !program_state.settlement_batch_hash.is_empty() {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    let mut netting_results: HashMap<String, i64> = HashMap::new();

    for token_settlements in &params.settlements {
        let increment_sum: u64 = token_settlements.clone().increments.into_iter().map(|x| x.amount).sum::<u64>() + token_settlements.fee_amount;
        let decrement_sum: u64 = token_settlements.clone().decrements.into_iter().map(|x| x.amount).sum::<u64>();
        let token_balance_state: TokenBalances = get_token_balance_state(accounts, token_settlements.account_index)?;
        verify_decrements(&token_balance_state, token_settlements.clone().decrements)?;
        let running_netting_total = netting_results.entry(token_balance_state.token_id).or_insert(0);
        *running_netting_total += increment_sum as i64 - decrement_sum as i64;
    }

    for (token, netting_result) in &netting_results {
        if *netting_result != 0 {
            msg!("Netting for {} - value is {}", token, netting_result);
            return Err(ProgramError::Custom(ERROR_NETTING));
        }
    }

    program_state.settlement_batch_hash = hash(borsh::to_vec(&params).unwrap());
    update_state_data(
        &accounts[0],
        borsh::to_vec(&program_state).unwrap()
    )
}

pub fn rollback_settlement_batch(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
    let mut program_state: ProgramState = get_account_state(accounts, 0)?;
    program_state.settlement_batch_hash = "".to_string();
    update_state_data(
        &accounts[0],
        borsh::to_vec(&program_state).unwrap()
    )
}


fn hash(data: Vec<u8>) -> String {
    digest(data).to_string()
}

fn update_state_data(account: &AccountInfo, new_data: Vec<u8>) -> Result<(), ProgramError> {
    if new_data.len() > entrypoint::MAX_PERMITTED_DATA_LENGTH as usize {
        return Err(ProgramError::InvalidRealloc);
    }
    account.realloc(new_data.len(), true)?;
    account.data.try_borrow_mut().unwrap().copy_from_slice(new_data.as_slice());
    Ok(())
}

fn get_account_state<T: borsh::BorshDeserialize>(accounts: &[AccountInfo], index: u8) -> Result<T, ProgramError> {
    let state_data = get_account_data(accounts, index)?;
    Ok(borsh::from_slice(&state_data).unwrap())
}

fn get_account_data(accounts: &[AccountInfo], index: u8) -> Result<Vec<u8>, ProgramError> {
    if index as usize >= accounts.len() {
        return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_INDEX));
    }
    Ok(accounts[index as usize].data.try_borrow().unwrap().to_vec())
}

fn get_token_balance_state(accounts: &[AccountInfo], index: u8) ->  Result<TokenBalances, ProgramError> {
    let token_balance_state: TokenBalances = get_account_state(accounts, index)?;
    if token_balance_state.program_state_account != *accounts[0].key {
        return Err(ProgramError::Custom(ERROR_PROGRAM_STATE_MISMATCH));
    }
    Ok(token_balance_state)
}

fn handle_increments(state: TokenBalances, adjustments: Vec<Adjustment>) -> Result<TokenBalances, ProgramError> {
    handle_adjustments(state, adjustments, true)
}

fn handle_decrements(state: TokenBalances, adjustments: Vec<Adjustment>) -> Result<TokenBalances, ProgramError> {
    handle_adjustments(state, adjustments, false)
}

fn handle_adjustments(mut state: TokenBalances, adjustments: Vec<Adjustment>, increment: bool) -> Result<TokenBalances, ProgramError> {
    for adjustment in adjustments {
        if adjustment.address_index as usize >= state.balances.len() {
            return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
        } else {
            let index = adjustment.address_index as usize;
            if increment {
                state.balances[index].balance += adjustment.amount
            } else {
                let current_balance = state.balances[index].balance;
                let new_balance = current_balance.checked_sub(adjustment.amount);
                state.balances[index].balance = match new_balance {
                    Some(new_balance) => new_balance,
                    None => return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE))
                };
            }
        }
    }
    Ok(state)
}

fn verify_decrements(state: &TokenBalances, adjustments: Vec<Adjustment>) -> Result<(), ProgramError>{
    for adjustment in adjustments {
        if adjustment.address_index as usize >= state.balances.len() {
            return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
        } else {
            let index = adjustment.address_index as usize;
            let current_balance = state.balances[index].balance;
            if adjustment.amount > current_balance {
                return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE));
            };
        }
    }
    Ok(())
}

fn handle_withdrawals(
    mut state: TokenBalances,
    withdrawals: Vec<Withdrawal>,
    fee_account_address: &str,
    tx_outs: &mut Vec<TxOut>,
    network_type: NetworkType,
) -> Result<TokenBalances, ProgramError> {
    for withdrawal in withdrawals {
        if withdrawal.address_index as usize >= state.balances.len() {
            return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
        } else {
            let index = withdrawal.address_index as usize;
            let current_balance = state.balances[index].balance;
            let new_balance = current_balance.checked_sub(withdrawal.amount);
            state.balances[index].balance = match new_balance {
                Some(new_balance) => new_balance,
                None => return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE))
            };
            if withdrawal.fee_amount > 0 {
                if state.balances[FEE_ADDRESS_INDEX as usize].address != fee_account_address {
                    return Err(ProgramError::Custom(ERROR_ADDRESS_MISMATCH))
                }
                state.balances[FEE_ADDRESS_INDEX as usize].balance += withdrawal.fee_amount
            }
            let message = create_bitcoin_withdrawal_message(
                withdrawal.amount,
                "BTC:0",
                &state.balances[index].address,
                withdrawal.timestamp
            );
            let bitcoin_address = BitcoinAddress::new(&state.balances[index].address).unwrap();
            if !BitcoinSignatureVerification::verify_message(
                &bitcoin_address,
                withdrawal.signature.as_slice(),
                message.as_slice()
            ) {
                return Err(ProgramError::Custom(ERROR_SIGNATURE_NOT_VALID))
            }

            tx_outs.push(
                TxOut {
                    value: Amount::from_sat(withdrawal.amount - withdrawal.fee_amount),
                    script_pubkey: get_bitcoin_address(
                        &state.balances[index].address,
                        network_type.clone()
                    ).script_pubkey(),
                }
            );
        }
    }
    Ok(state)
}

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
