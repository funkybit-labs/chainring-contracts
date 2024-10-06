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
use std::convert::TryInto;

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
const ERROR_INVALID_SIGNER: u32 = 613;
const ERROR_VALUE_TOO_LONG: u32 = 613;


entrypoint!(process_instruction);
pub fn process_instruction(
    program_id: &Pubkey,
    accounts: &[AccountInfo],
    instruction_data: &[u8],
) -> Result<(), ProgramError> {

    let exchange_instruction: ProgramInstruction = borsh::from_slice(instruction_data).unwrap();
    match exchange_instruction {
        ProgramInstruction::InitProgramState(params) =>  init_program_state(accounts, &params),
        ProgramInstruction::InitTokenState(params) =>  init_token_state(accounts, &params),
        ProgramInstruction::InitWalletBalances(params) =>  init_wallet_balances(accounts, &params),
        ProgramInstruction::BatchDeposit(params) => deposit_batch(accounts, &params),
        ProgramInstruction::BatchWithdraw(params) => withdraw_batch(program_id, accounts, &params),
        ProgramInstruction::SubmitBatchSettlement(params) => submit_settlement_batch(accounts, &params),
        ProgramInstruction::PrepareBatchSettlement(params) => prepare_settlement_batch(accounts, &params),
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

impl NetworkType {
    pub fn to_vec(&self) -> Vec<u8> {
        vec![
            match self {
                Self::Bitcoin => 0,
                Self::Testnet => 1,
                Self::Signet => 2,
                Self::Regtest => 3
            }
        ]
    }

    pub fn from_u8(data: u8) -> Self {
        match data {
            0 => Self::Bitcoin,
            1 => Self::Testnet,
            2 => Self::Signet,
            3 => Self::Regtest,
            _ => Self::Bitcoin
        }
    }
}

const VERSION_SIZE: usize = 4;
const PUBKEY_SIZE: usize =  32;
const PROGRAM_PUBKEY_OFFSET: usize = VERSION_SIZE;
const MAX_TOKEN_ID_SIZE: usize = 32;
const TOKEN_ID_OFFSET: usize = PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE;
const BALANCE_COUNT_SIZE: usize = 4;
const BALANCE_COUNT_OFFSET: usize = TOKEN_ID_OFFSET + MAX_TOKEN_ID_SIZE;
const TOKEN_STATE_HEADER_SIZE: usize = VERSION_SIZE + PUBKEY_SIZE + MAX_TOKEN_ID_SIZE + BALANCE_COUNT_SIZE;


const MAX_ADDRESS_SIZE: usize = 92;
const BALANCE_AMOUNT_SIZE: usize = 8;
const BALANCE_AMOUNT_OFFSET: usize = MAX_ADDRESS_SIZE;
const BALANCE_SIZE: usize  = MAX_ADDRESS_SIZE + BALANCE_AMOUNT_SIZE;

const NETWORK_TYPE_SIZE: usize = 1;
const HASH_SIZE: usize = 32;

const FEE_ACCOUNT_OFFSET: usize = VERSION_SIZE;
const PROGRAM_CHANGE_ADDRESS_OFFSET: usize = FEE_ACCOUNT_OFFSET + MAX_ADDRESS_SIZE;
const NETWORK_TYPE_OFFSET: usize = PROGRAM_CHANGE_ADDRESS_OFFSET + MAX_ADDRESS_SIZE;

const SETTLEMENT_HASH_OFFSET: usize  = NETWORK_TYPE_OFFSET + NETWORK_TYPE_SIZE;
const LAST_SETTLEMENT_HASH_OFFSET: usize = SETTLEMENT_HASH_OFFSET + HASH_SIZE;
const LAST_WITHDRAWAL_HASH_OFFSET: usize = LAST_SETTLEMENT_HASH_OFFSET + HASH_SIZE;

#[derive(Clone, Debug)]
pub struct Balance {
    pub address: String,
    pub balance: u64,
}

#[derive(Clone, Debug)]
pub struct TokenState {
    pub version: u32,
    pub program_state_account: Pubkey,
    pub token_id: String,
    pub balances: Vec<Balance>,
}

impl TokenState {
    pub fn initialize(account: &AccountInfo, token_id: &str, fee_account_address: &str, pubkey: &Pubkey) -> Result<(), ProgramError> {
        Self::grow_balance_accounts_if_needed(account, 1)?;
        Self::set_program_account(account, pubkey)?;
        set_string(account, TOKEN_ID_OFFSET, token_id, MAX_TOKEN_ID_SIZE)?;
        Self::set_num_balances(account, 1)?;
        Balance::set_wallet_address(account, 0, fee_account_address)
    }

    pub fn get_num_balances(account: &AccountInfo) -> Result<usize, ProgramError> {
        let offset = BALANCE_COUNT_OFFSET;
        Ok(u32::from_le_bytes(
            account.data.borrow()[offset..offset+4]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?
        ) as usize)
    }

    pub fn set_num_balances(account: &AccountInfo, num_balances: usize) -> Result<(), ProgramError> {
        let offset = BALANCE_COUNT_OFFSET;
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[offset..offset+4].copy_from_slice((num_balances as u32).to_le_bytes().as_slice()))
    }

    pub fn get_token_id(account: &AccountInfo) -> Result<String, ProgramError> {
        let mut tmp = [0u8; MAX_TOKEN_ID_SIZE];
        tmp[..MAX_TOKEN_ID_SIZE].copy_from_slice(&account.data.borrow()[TOKEN_ID_OFFSET..TOKEN_ID_OFFSET+MAX_TOKEN_ID_SIZE]);
        let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_TOKEN_ID_SIZE);
        String::from_utf8(tmp[..pos].to_vec()).map_err(|_| ProgramError::InvalidAccountData)
    }

    pub fn get_program_state_account_key(account: &AccountInfo) -> Result<Pubkey, ProgramError> {
        Ok(Pubkey::from_slice(account.data.borrow()[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET+PUBKEY_SIZE]
            .try_into().map_err(|_| ProgramError::InvalidAccountData)?))
    }

    fn validate_account(accounts: &[AccountInfo], index: u8) -> Result<(), ProgramError> {
        if index as usize >= accounts.len() {
            return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_INDEX));
        }
        if TokenState::get_program_state_account_key(&accounts[index as usize])? != *accounts[0].key {
            return Err(ProgramError::Custom(ERROR_PROGRAM_STATE_MISMATCH));
        }
        Ok(())
    }

    fn grow_balance_accounts_if_needed(account: &AccountInfo, additional_balances: usize) -> Result<(), ProgramError> {
        let original_data_len = unsafe { account.original_data_len() };

        let num_balances = if original_data_len > 0 {
            TokenState::get_num_balances(account)?
        } else {
            0
        };
        if TOKEN_STATE_HEADER_SIZE + (num_balances + additional_balances) * BALANCE_SIZE > original_data_len {
            msg!("Growing token account");
            account.realloc(original_data_len + entrypoint::MAX_PERMITTED_DATA_INCREASE as usize, true)?
        }
        Ok(())
    }

    fn set_program_account(account: &AccountInfo, pubkey: &Pubkey) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET+PUBKEY_SIZE].copy_from_slice(
            pubkey.0.as_slice()
        ))

    }

}

impl Balance {

    pub fn get_wallet_balance(account: &AccountInfo, index: usize) -> Result<u64, ProgramError>  {
        let offset = TOKEN_STATE_HEADER_SIZE + index * BALANCE_SIZE + BALANCE_AMOUNT_OFFSET;
        Ok(u64::from_le_bytes(
            account.data.borrow()[offset..offset+8]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?
        ))
    }

    pub fn set_wallet_balance(account: &AccountInfo, index: usize, balance: u64) -> Result<(), ProgramError> {
        let offset = TOKEN_STATE_HEADER_SIZE + index * BALANCE_SIZE + BALANCE_AMOUNT_OFFSET;
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[offset..offset+8].copy_from_slice(
            balance.to_le_bytes().as_slice()
        ))
    }

    pub fn adjust_wallet_balance(account: &AccountInfo, index: usize, balance_adjustment: u64) -> Result<(), ProgramError> {
        let current_balance = Self::get_wallet_balance(account, index)?;
        Self::set_wallet_balance(account, index, current_balance + balance_adjustment)
    }

    pub fn get_wallet_address(account: &AccountInfo, index: usize) -> Result<String, ProgramError> {
        get_address(account, TOKEN_STATE_HEADER_SIZE + index * BALANCE_SIZE)
    }

    pub fn set_wallet_address(account: &AccountInfo, index: usize, address: &str) -> Result<(), ProgramError> {
        set_string(account, TOKEN_STATE_HEADER_SIZE + index * BALANCE_SIZE, address, MAX_ADDRESS_SIZE)
    }
}

#[derive(Clone)]
pub struct ProgramState {
    pub version: u32,
    pub fee_account_address: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
    pub settlement_batch_hash: [u8; 32],
    pub last_settlement_batch_hash: [u8; 32],
    pub last_withdrawal_batch_hash: [u8; 32],
}

impl ProgramState {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut serialized = vec![];
        serialized.extend(self.version.to_le_bytes());
        let mut tmp = [0u8; MAX_ADDRESS_SIZE];
        let bytes = self.fee_account_address.as_bytes();
        tmp[..bytes.len()].copy_from_slice(self.fee_account_address.as_bytes());
        serialized.extend(tmp.as_slice());
        let bytes = self.program_change_address.as_bytes();
        tmp[..bytes.len()].copy_from_slice(bytes);
        serialized.extend(tmp.as_slice());
        serialized.extend(self.network_type.to_vec());
        serialized.extend(self.settlement_batch_hash.as_slice());
        serialized.extend(self.last_settlement_batch_hash.as_slice());
        serialized.extend(self.last_withdrawal_batch_hash.as_slice());
        serialized
    }

    pub fn get_fee_account_address(account: &AccountInfo) -> Result<String, ProgramError> {
        get_address(account, FEE_ACCOUNT_OFFSET)
    }

    pub fn get_program_change_address(account: &AccountInfo) -> Result<String, ProgramError> {
        get_address(account, PROGRAM_CHANGE_ADDRESS_OFFSET)
    }

    pub fn get_network_type(account: &AccountInfo) -> NetworkType {
        NetworkType::from_u8(account.data.borrow()[NETWORK_TYPE_OFFSET])
    }

    pub fn get_settlement_hash(account: &AccountInfo) -> Result<[u8; 32], ProgramError> {
        hash_from_slice(account, SETTLEMENT_HASH_OFFSET)
    }

    pub fn clear_settlement_hash(account: &AccountInfo) -> Result<(), ProgramError> {
        Self::set_settlement_hash(account, [0u8; HASH_SIZE])
    }

    pub fn set_settlement_hash(account: &AccountInfo, hash: [u8; 32]) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[SETTLEMENT_HASH_OFFSET..SETTLEMENT_HASH_OFFSET+HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn set_last_settlement_hash(account: &AccountInfo, hash: [u8; 32]) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[LAST_SETTLEMENT_HASH_OFFSET..LAST_SETTLEMENT_HASH_OFFSET+HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn set_last_withdrawal_hash(account: &AccountInfo, hash: [u8; 32]) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[LAST_WITHDRAWAL_HASH_OFFSET..LAST_WITHDRAWAL_HASH_OFFSET+HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    fn validate_signer(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
        if !accounts[0].is_signer {
            return Err(ProgramError::Custom(ERROR_INVALID_SIGNER));
        }
        Ok(())
    }

}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Adjustment {
    pub address_index: u32,
    pub amount: u64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct TokenStateetup {
    pub account_index: u8,
    pub wallet_addresses: Vec<String>,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Withdrawal {
    pub address_index: u32,
    pub amount: u64,
    pub fee_amount: u64,
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
    pub token_balance_setups: Vec<TokenStateetup>,
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
        settlement_batch_hash: [0u8; 32],
        last_settlement_batch_hash: [0u8; 32],
        last_withdrawal_batch_hash: [0u8; 32],
    }.to_vec())
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
    for token_balance_setup in &params.token_balance_setups {
        TokenState::validate_account(accounts, token_balance_setup.account_index)?;
        let account = &accounts[token_balance_setup.account_index as usize];
        TokenState::grow_balance_accounts_if_needed(account, token_balance_setup.wallet_addresses.len())?;
        let mut num_balances = TokenState::get_num_balances(account)?;
        for wallet_address in &token_balance_setup.wallet_addresses {
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
    if ProgramState::get_settlement_hash(&accounts[0])? != [0u8; 32] {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_IN_PROGRESS));
    }
    for token_deposits in &params.token_deposits {
        TokenState::validate_account(accounts, token_deposits.account_index)?;
        handle_increments(&accounts[token_deposits.account_index as usize], token_deposits.clone().deposits)?;
    }
    Ok(())
}

pub fn withdraw_batch(program_id: &Pubkey, accounts: &[AccountInfo], params: &WithdrawBatchParams) -> Result<(), ProgramError> {
    if ProgramState::get_settlement_hash(&accounts[0])? != [0u8; 32] {
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

    ProgramState::set_last_withdrawal_hash(&accounts[0], hash(borsh::to_vec(&params).unwrap()))?;

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

pub fn submit_settlement_batch(accounts: &[AccountInfo], params: &SettlementBatchParams) -> Result<(), ProgramError> {

    let current_hash = ProgramState::get_settlement_hash(&accounts[0])?;
    let params_hash = hash(borsh::to_vec(&params).unwrap());

    if current_hash == [0u8; 32] {
        return Err(ProgramError::Custom(ERROR_NO_SETTLEMENT_IN_PROGRESS));
    }
    if current_hash != params_hash {
        return Err(ProgramError::Custom(ERROR_SETTLEMENT_BATCH_MISMATCH));
    }

    for token_settlements in &params.settlements {
        TokenState::validate_account(accounts, token_settlements.account_index)?;
        let mut increments = if token_settlements.fee_amount > 0 {
            vec![Adjustment {
                address_index: FEE_ADDRESS_INDEX,
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

pub fn prepare_settlement_batch(accounts: &[AccountInfo], params: &SettlementBatchParams) -> Result<(), ProgramError> {

    if ProgramState::get_settlement_hash(&accounts[0])? != [0u8; 32] {
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

    ProgramState::set_settlement_hash(&accounts[0], hash(borsh::to_vec(&params).unwrap()))
}

pub fn rollback_settlement_batch(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
    ProgramState::clear_settlement_hash(&accounts[0])
}

fn hash(data: Vec<u8>) -> [u8; 32] {
    let mut tmp = [0u8; 32];
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
        if adjustment.address_index as usize >= TokenState::get_num_balances(account)? {
            return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
        } else {
            let index = adjustment.address_index as usize;
            let mut current_balance = Balance::get_wallet_balance(account, index)?;
            if increment {
                current_balance += adjustment.amount
            } else {
                let new_balance = current_balance.checked_sub(adjustment.amount);
                current_balance = match new_balance {
                    Some(new_balance) => new_balance,
                    None => return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE))
                };
            }
            Balance::set_wallet_balance(account, index, current_balance)?
        }
    }
    Ok(())
}

fn verify_decrements(account: &AccountInfo, adjustments: Vec<Adjustment>) -> Result<(), ProgramError>{
    for adjustment in adjustments {
        if adjustment.address_index as usize >= TokenState::get_num_balances(account)? {
            return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
        } else {
            let index = adjustment.address_index as usize;
            let current_balance = Balance::get_wallet_balance(account, index)?;
            if adjustment.amount > current_balance {
                return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE));
            };
        }
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
        if withdrawal.address_index as usize >= TokenState::get_num_balances(account)? {
            return Err(ProgramError::Custom(ERROR_INVALID_ADDRESS_INDEX))
        } else {
            let index = withdrawal.address_index as usize;
            let mut current_balance = Balance::get_wallet_balance(account, index)?;
            let new_balance = current_balance.checked_sub(withdrawal.amount);
            current_balance = match new_balance {
                Some(new_balance) => new_balance,
                None => return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE))
            };
            if withdrawal.fee_amount > 0 {
                if Balance::get_wallet_address(account, FEE_ADDRESS_INDEX as usize)? != fee_account_address {
                    return Err(ProgramError::Custom(ERROR_ADDRESS_MISMATCH))
                }
                Balance::adjust_wallet_balance(account, FEE_ADDRESS_INDEX as usize, withdrawal.fee_amount)?;
            }
            Balance::set_wallet_balance(account, index, current_balance)?;
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

fn get_address(account: &AccountInfo, offset: usize) -> Result<String, ProgramError> {
    let mut tmp = [0u8; MAX_ADDRESS_SIZE];
    tmp[..MAX_ADDRESS_SIZE].copy_from_slice(&account.data.borrow()[offset..offset+MAX_ADDRESS_SIZE]);
    let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_ADDRESS_SIZE);
    String::from_utf8(tmp[..pos].to_vec()).map_err(|_| ProgramError::InvalidAccountData)
}

pub fn set_string(account: &AccountInfo, offset: usize, string: &str, max_size: usize) -> Result<(), ProgramError> {
    let bytes = string.as_bytes();
    if bytes.len() >= max_size {
        return Err(ProgramError::Custom(ERROR_VALUE_TOO_LONG));
    }
    let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(data[offset..offset+bytes.len()].copy_from_slice(bytes))
}

fn hash_from_slice(account: &AccountInfo, offset: usize) -> Result<[u8; 32], ProgramError> {
    let mut tmp = [0u8; HASH_SIZE];
    tmp[..HASH_SIZE].copy_from_slice(account.data.borrow()[offset..offset+HASH_SIZE]
        .try_into().map_err(|_| ProgramError::InvalidAccountData)?);
    Ok(tmp)
}