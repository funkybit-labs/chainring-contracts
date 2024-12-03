use std::convert::TryInto;
use std::str::FromStr;
use arch_program::{
    account::AccountInfo,
    entrypoint,
    pubkey::Pubkey,
    program_error::ProgramError,
};
use bitcoin::Address;
use crate::error::*;
use crate::serialization::Codable;
use ordinals::RuneId;

pub const ACCOUNT_TYPE_SIZE: usize = 1;
pub const VERSION_SIZE: usize = 4;
pub const PUBKEY_SIZE: usize = 32;
pub const PROGRAM_PUBKEY_OFFSET: usize = VERSION_SIZE + ACCOUNT_TYPE_SIZE;
pub const MAX_TOKEN_ID_SIZE: usize = 32;
pub const TOKEN_ID_OFFSET: usize = PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE;
pub const BALANCE_COUNT_SIZE: usize = 4;
pub const BALANCE_COUNT_OFFSET: usize = TOKEN_ID_OFFSET + MAX_TOKEN_ID_SIZE;
pub const BALANCES_OFFSET: usize = BALANCE_COUNT_OFFSET + BALANCE_COUNT_SIZE;


pub const MAX_ADDRESS_SIZE: usize = 92;
pub const BALANCE_AMOUNT_SIZE: usize = 8;
pub const BALANCE_AMOUNT_OFFSET: usize = MAX_ADDRESS_SIZE;
pub const BALANCE_SIZE: usize = MAX_ADDRESS_SIZE + BALANCE_AMOUNT_SIZE;

pub const NETWORK_TYPE_SIZE: usize = 1;
pub const HASH_SIZE: usize = 32;

pub const WITHDRAW_ACCOUNT_PUBKEY_OFFSET: usize = VERSION_SIZE + ACCOUNT_TYPE_SIZE;
pub const FEE_ACCOUNT_OFFSET: usize = WITHDRAW_ACCOUNT_PUBKEY_OFFSET + PUBKEY_SIZE;
pub const PROGRAM_CHANGE_ADDRESS_OFFSET: usize = FEE_ACCOUNT_OFFSET + MAX_ADDRESS_SIZE;
pub const NETWORK_TYPE_OFFSET: usize = PROGRAM_CHANGE_ADDRESS_OFFSET + MAX_ADDRESS_SIZE;

pub const SETTLEMENT_HASH_OFFSET: usize = NETWORK_TYPE_OFFSET + NETWORK_TYPE_SIZE;
pub const LAST_SETTLEMENT_HASH_OFFSET: usize = SETTLEMENT_HASH_OFFSET + HASH_SIZE;
pub const EVENTS_SIZE_OFFSET: usize = LAST_SETTLEMENT_HASH_OFFSET + HASH_SIZE;
pub const EVENTS_OFFSET: usize = EVENTS_SIZE_OFFSET + 2;
pub const EVENT_SIZE: usize = 64;
pub const MAX_EVENTS: usize = 100;
pub const RUNE_RECEIVER_OFFSET: usize = EVENTS_OFFSET + EVENT_SIZE * MAX_EVENTS;

pub const FEE_ADDRESS_INDEX: u32 = 0;

pub const EMPTY_HASH: [u8; 32] = [0u8; 32];

pub const DUST_THRESHOLD: u64 = 546;

pub type Hash = [u8; 32];
pub type WalletLast4 = [u8; 4];

#[derive(Clone, PartialEq, Debug)]
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


#[derive(Clone, Debug, PartialEq)]
pub struct Balance {
    pub address: String,
    pub balance: u64,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Event {
    FailedSettlement {
        account_index: u8,
        address_index: u32,
        requested_amount: u64,
        balance: u64,
        error_code: u32,
    },
    FailedWithdrawal {
        account_index: u8,
        address_index: u32,
        fee_account_index: u8,
        fee_address_index: u32,
        requested_amount: u64,
        fee_amount: u64,
        balance: u64,
        balance_in_fee_token: u64,
        error_code: u32,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum AccountType {
    Program,
    Token,
    Withdraw,
    RuneReceiver,
    Unknown
}

#[derive(Clone, Debug, PartialEq)]
pub struct TokenState {
    pub account_type: AccountType,
    pub version: u32,
    pub program_state_account: Pubkey,
    pub token_id: String,
    pub balances: Vec<Balance>,
}

#[derive(Clone, Debug)]
pub struct ProgramState {
    pub account_type: AccountType,
    pub version: u32,
    pub withdraw_account: Pubkey,
    pub fee_account_address: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
    pub settlement_batch_hash: Hash,
    pub last_settlement_batch_hash: Hash,
    pub events: Vec<Event>,
}

#[derive(Clone, Debug)]
pub struct WithdrawState {
    pub account_type: AccountType,
    pub version: u32,
    pub program_state_account: Pubkey,
    pub batch_hash: Hash,
}

#[derive(Clone, Debug)]
pub struct RuneReceiverState {
    pub account_type: AccountType,
    pub version: u32,
    pub program_state_account: Pubkey,
}

impl TokenState {
    pub fn initialize(account: &AccountInfo, token_id: &str, fee_account_address: &str, pubkey: &Pubkey) -> Result<(), ProgramError> {
        Self::grow_balance_accounts_if_needed(account, 1)?;
        set_type(account, AccountType::Token)?;
        Self::set_program_account(account, pubkey)?;
        set_string(account, TOKEN_ID_OFFSET, token_id, MAX_TOKEN_ID_SIZE)?;
        if !Self::is_rune_id(token_id) {
            Self::set_num_balances(account, 1)?;
            Balance::set_wallet_address(account, 0, fee_account_address)
        } else {
            Ok(())
        }
    }

    pub fn get_num_balances(account: &AccountInfo) -> Result<usize, ProgramError> {
        let offset = BALANCE_COUNT_OFFSET;
        Ok(u32::from_le_bytes(
            account.data.borrow()[offset..offset + 4]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?
        ) as usize)
    }

    pub fn set_num_balances(account: &AccountInfo, num_balances: usize) -> Result<(), ProgramError> {
        let offset = BALANCE_COUNT_OFFSET;
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[offset..offset + 4].copy_from_slice((num_balances as u32).to_le_bytes().as_slice()))
    }

    pub fn get_token_id(account: &AccountInfo) -> Result<String, ProgramError> {
        let mut tmp = [0u8; MAX_TOKEN_ID_SIZE];
        tmp[..MAX_TOKEN_ID_SIZE].copy_from_slice(&account.data.borrow()[TOKEN_ID_OFFSET..TOKEN_ID_OFFSET + MAX_TOKEN_ID_SIZE]);
        let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_TOKEN_ID_SIZE);
        String::from_utf8(tmp[..pos].to_vec()).map_err(|_| ProgramError::InvalidAccountData)
    }

    pub fn get_rune_id(account: &AccountInfo) -> Result<RuneId, ProgramError> {
        let token_id = Self::get_token_id(account)?;
        RuneId::from_str(&token_id).map_err(|_| ProgramError::InvalidAccountData)
    }

    pub fn is_rune_account(account: &AccountInfo) -> bool {
        let token_id = Self::get_token_id(account).unwrap();
        Self::is_rune_id(&token_id)
    }

    pub fn is_rune_id(token_id: &str) -> bool {
        match RuneId::from_str(&token_id) {
            Ok(_) => true,
            _ => false,
        }
    }

    pub fn get_program_state_account_key(account: &AccountInfo) -> Result<Pubkey, ProgramError> {
        Ok(Pubkey::from_slice(account.data.borrow()[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE]
            .try_into().map_err(|_| ProgramError::InvalidAccountData)?))
    }

    pub fn grow_balance_accounts_if_needed(account: &AccountInfo, additional_balances: usize) -> Result<(), ProgramError> {
        let original_data_len = unsafe { account.original_data_len() };

        let num_balances = if original_data_len > 0 {
            TokenState::get_num_balances(account)?
        } else {
            0
        };
        if BALANCES_OFFSET + (num_balances + additional_balances) * BALANCE_SIZE > original_data_len {
            account.realloc(original_data_len + entrypoint::MAX_PERMITTED_DATA_INCREASE, true)?
        }
        Ok(())
    }

    fn set_program_account(account: &AccountInfo, pubkey: &Pubkey) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE].copy_from_slice(
            pubkey.0.as_slice()
        ))
    }
}

impl Balance {
    pub fn get_wallet_balance(account: &AccountInfo, index: usize) -> Result<u64, ProgramError> {
        let offset = BALANCES_OFFSET + index * BALANCE_SIZE + BALANCE_AMOUNT_OFFSET;
        Ok(u64::from_le_bytes(
            account.data.borrow()[offset..offset + 8]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?
        ))
    }

    pub fn set_wallet_balance(account: &AccountInfo, index: usize, balance: u64) -> Result<(), ProgramError> {
        let offset = BALANCES_OFFSET + index * BALANCE_SIZE + BALANCE_AMOUNT_OFFSET;
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[offset..offset + 8].copy_from_slice(
            balance.to_le_bytes().as_slice()
        ))
    }

    pub fn increment_wallet_balance(account: &AccountInfo, index: usize, balance_adjustment: u64) -> Result<(), ProgramError> {
        let current_balance = Self::get_wallet_balance(account, index)?;
        Self::set_wallet_balance(account, index, current_balance + balance_adjustment)
    }

    pub fn decrement_wallet_balance(account: &AccountInfo, index: usize, balance_adjustment: u64) -> Result<(), ProgramError> {
        let mut current_balance = Self::get_wallet_balance(account, index)?;
        let new_balance = current_balance.checked_sub(balance_adjustment);
        current_balance = match new_balance {
            Some(new_balance) => new_balance,
            None => return Err(ProgramError::Custom(ERROR_INSUFFICIENT_BALANCE))
        };
        Self::set_wallet_balance(account, index, current_balance)
    }

    pub fn get_wallet_address(account: &AccountInfo, index: usize) -> Result<String, ProgramError> {
        get_address(account, BALANCES_OFFSET + index * BALANCE_SIZE)
    }

    pub fn set_wallet_address(account: &AccountInfo, index: usize, address: &str) -> Result<(), ProgramError> {
        set_string(account, BALANCES_OFFSET + index * BALANCE_SIZE, address, MAX_ADDRESS_SIZE)
    }

    pub fn get_wallet_address_last4(account: &AccountInfo, index: usize) -> Result<WalletLast4, ProgramError> {
        Ok(wallet_last4(&Self::get_wallet_address(account, index)?))
    }
}

impl ProgramState {

    pub fn get_withdraw_account_key(account: &AccountInfo) -> Result<Pubkey, ProgramError> {
        Ok(Pubkey::from_slice(account.data.borrow()[WITHDRAW_ACCOUNT_PUBKEY_OFFSET..WITHDRAW_ACCOUNT_PUBKEY_OFFSET + PUBKEY_SIZE]
            .try_into().map_err(|_| ProgramError::InvalidAccountData)?))
    }

    pub fn get_fee_account_address(account: &AccountInfo) -> Result<String, ProgramError> {
        get_address(account, FEE_ACCOUNT_OFFSET)
    }

    pub fn get_program_change_address(account: &AccountInfo) -> Result<String, ProgramError> {
        get_address(account, PROGRAM_CHANGE_ADDRESS_OFFSET)
    }

    pub fn get_network_type(account: &AccountInfo) -> NetworkType {
        NetworkType::decode_from_slice(&account.data.borrow()[NETWORK_TYPE_OFFSET..]).unwrap()
    }

    pub fn get_settlement_hash(account: &AccountInfo) -> Result<Hash, ProgramError> {
        hash_from_slice(account, SETTLEMENT_HASH_OFFSET)
    }

    pub fn clear_settlement_hash(account: &AccountInfo) -> Result<(), ProgramError> {
        Self::set_settlement_hash(account, [0u8; HASH_SIZE])
    }

    pub fn set_settlement_hash(account: &AccountInfo, hash: Hash) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[SETTLEMENT_HASH_OFFSET..SETTLEMENT_HASH_OFFSET + HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn set_last_settlement_hash(account: &AccountInfo, hash: Hash) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[LAST_SETTLEMENT_HASH_OFFSET..LAST_SETTLEMENT_HASH_OFFSET + HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn clear_events(account: &AccountInfo) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[EVENTS_SIZE_OFFSET..EVENTS_SIZE_OFFSET + 2].copy_from_slice(0u16.to_le_bytes().as_slice()))
    }

    pub fn get_events_count(account: &AccountInfo) -> Result<usize, ProgramError> {
        Ok(u16::from_le_bytes(
            account.data.borrow()[EVENTS_SIZE_OFFSET..EVENTS_SIZE_OFFSET + 2]
                .try_into()
                .map_err(|_| ProgramError::InvalidAccountData)?
        ) as usize)
    }

    pub fn emit_event(account: &AccountInfo, event: &Event) -> Result<(), ProgramError> {
        let current_count = Self::get_events_count(account)?;
        if current_count == MAX_EVENTS {
            return Err(ProgramError::Custom(ERROR_VALUE_TOO_LARGE));
        }
        let offset = EVENTS_OFFSET + (current_count * EVENT_SIZE);
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        data[offset..offset + EVENT_SIZE].copy_from_slice(
            event.encode_to_vec().unwrap().as_slice()
        );
        Ok(data[EVENTS_SIZE_OFFSET..EVENTS_SIZE_OFFSET + 2].copy_from_slice(((current_count + 1) as u16).to_le_bytes().as_slice()))
    }

    pub fn get_failed_withdrawal_amount(account: &AccountInfo) -> Result<u64, ProgramError> {
        let current_count = Self::get_events_count(account)? as usize;
        let mut amount: u64 = 0;
        let data = account.data.borrow();
        for i in 0..current_count {
            let offset = EVENTS_OFFSET + i * EVENT_SIZE;
            let event = Event::decode_from_slice(
                data[offset..offset + EVENT_SIZE].try_into().map_err(|_| ProgramError::InvalidAccountData)?
            ).map_err(|_| ProgramError::InvalidAccountData)?;
            match event {
                Event::FailedWithdrawal { requested_amount, fee_amount, .. } => {
                    amount += requested_amount - fee_amount
                }
                Event::FailedSettlement { .. } => {}
            }
        }
        Ok(amount)
    }

    fn set_rune_receiver(account: &AccountInfo, pubkey: &Pubkey) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[RUNE_RECEIVER_OFFSET..RUNE_RECEIVER_OFFSET + PUBKEY_SIZE].copy_from_slice(
            pubkey.0.as_slice()
        ))
    }
}

pub const WITHDRAW_HASH_OFFSET: usize = PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE;
pub const WITHDRAW_ACCOUNT_SIZE: usize = WITHDRAW_HASH_OFFSET + HASH_SIZE;
impl WithdrawState {

    pub fn initialize(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
        if accounts[1].data_is_empty() {
            accounts[1].realloc(WITHDRAW_ACCOUNT_SIZE, true)?;
            set_type(&accounts[1], AccountType::Withdraw)?;
            Self::set_program_account(&accounts[1], accounts[0].key)
        } else {
            Ok(())
        }
    }

    pub fn get_program_state_account_key(account: &AccountInfo) -> Result<Pubkey, ProgramError> {
        Ok(Pubkey::from_slice(account.data.borrow()[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE]
            .try_into().map_err(|_| ProgramError::InvalidAccountData)?))
    }

    fn set_program_account(account: &AccountInfo, pubkey: &Pubkey) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE].copy_from_slice(
            pubkey.0.as_slice()
        ))
    }

    pub fn get_hash(account: &AccountInfo) -> Result<Hash, ProgramError> {
        hash_from_slice(account, WITHDRAW_HASH_OFFSET)
    }

    pub fn clear_hash(account: &AccountInfo) -> Result<(), ProgramError> {
        Self::set_hash(account, [0u8; HASH_SIZE])
    }

    pub fn set_hash(account: &AccountInfo, hash: Hash) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[WITHDRAW_HASH_OFFSET..WITHDRAW_HASH_OFFSET + HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }
}

pub const RUNE_RECEIVER_ACCOUNT_SIZE: usize = PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE;
impl RuneReceiverState {

    pub fn initialize(accounts: &[AccountInfo], account_index: usize) -> Result<(), ProgramError> {
        if accounts[1].data_is_empty() {
            accounts[1].realloc(RUNE_RECEIVER_ACCOUNT_SIZE, true)?;
            set_type(&accounts[account_index], AccountType::RuneReceiver)?;
            Self::set_program_account(&accounts[account_index], accounts[0].key)?;
            if accounts[0].data_len() == RUNE_RECEIVER_OFFSET {
                accounts[0].realloc(RUNE_RECEIVER_OFFSET + PUBKEY_SIZE, true)?;
                ProgramState::set_rune_receiver(&accounts[0], accounts[1].key)
            } else {
                Ok(())
            }
        } else {
            Ok(())
        }
    }

    pub fn get_program_state_account_key(account: &AccountInfo) -> Result<Pubkey, ProgramError> {
        Ok(Pubkey::from_slice(account.data.borrow()[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE]
            .try_into().map_err(|_| ProgramError::InvalidAccountData)?))
    }

    fn set_program_account(account: &AccountInfo, pubkey: &Pubkey) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[PROGRAM_PUBKEY_OFFSET..PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE].copy_from_slice(
            pubkey.0.as_slice()
        ))
    }
}

pub fn set_type(account: &AccountInfo, account_type: AccountType) -> Result<(), ProgramError> {
    let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(data[0..1].copy_from_slice(account_type.encode_to_vec().unwrap().as_slice()))
}

pub fn get_type(account: &AccountInfo) -> Result<AccountType, ProgramError> {
    Ok(AccountType::decode_from_slice(&account.data.borrow()[0..1]).map_err(|_| ProgramError::InvalidAccountData)?)
}

fn get_address(account: &AccountInfo, offset: usize) -> Result<String, ProgramError> {
    let mut tmp = [0u8; MAX_ADDRESS_SIZE];
    tmp[..MAX_ADDRESS_SIZE].copy_from_slice(&account.data.borrow()[offset..offset + MAX_ADDRESS_SIZE]);
    let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_ADDRESS_SIZE);
    String::from_utf8(tmp[..pos].to_vec()).map_err(|_| ProgramError::InvalidAccountData)
}

pub fn set_string(account: &AccountInfo, offset: usize, string: &str, max_size: usize) -> Result<(), ProgramError> {
    let bytes = string.as_bytes();
    if bytes.len() >= max_size {
        return Err(ProgramError::Custom(ERROR_VALUE_TOO_LARGE));
    }
    let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(data[offset..offset + bytes.len()].copy_from_slice(bytes))
}

fn hash_from_slice(account: &AccountInfo, offset: usize) -> Result<Hash, ProgramError> {
    let mut tmp = EMPTY_HASH;
    tmp[..HASH_SIZE].copy_from_slice(account.data.borrow()[offset..offset + HASH_SIZE]
        .try_into().map_err(|_| ProgramError::InvalidAccountData)?);
    Ok(tmp)
}

pub fn wallet_last4(address: &str) -> WalletLast4 {
    let mut tmp: WalletLast4 = [0u8; 4];
    tmp[0..4].copy_from_slice(&address.as_bytes()[address.len() - 4..address.len()]);
    tmp
}

pub fn validate_account(accounts: &[AccountInfo], index: u8, is_signer: bool, is_writable: bool, account_type: Option<AccountType>, related_account_index: Option<u8>) -> Result<(), ProgramError> {
    if index as usize >= accounts.len() {
        return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_INDEX));
    }
    let account = &accounts[index as usize];
    if account.is_signer != is_signer || account.is_writable != is_writable {
        return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_FLAGS));
    }
    if let Some(account_type) = account_type {
        if get_type(account)? != account_type {
            return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_TYPE));
        }
        if let Some(related_account_index) = related_account_index{
            let related_key = match account_type {
                AccountType::Program => ProgramState::get_withdraw_account_key(&account),
                AccountType::Withdraw => WithdrawState::get_program_state_account_key(&account),
                AccountType::Token => TokenState::get_program_state_account_key(&account),
                AccountType::RuneReceiver => RuneReceiverState::get_program_state_account_key(&account),
                _ => Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_TYPE))
            }?;
            if related_key != *accounts[related_account_index as usize].key {
                return Err(ProgramError::Custom(ERROR_STATE_ACCOUNT_MISMATCH));
            }
        }
    } else {
        if !account.data_is_empty() {
            return Err(ProgramError::Custom(ERROR_ALREADY_INITIALIZED))
        }
    }
    Ok(())
}

pub fn validate_bitcoin_address(address: &str, network_type: NetworkType, strict: bool) -> Result<(), ProgramError> {
    let network_unchecked_address = Address::from_str(address)
        .map_err(|_| ProgramError::Custom(ERROR_INVALID_ADDRESS))?;

    if strict || network_type == NetworkType::Bitcoin {
        network_unchecked_address
            .require_network(map_network_type(network_type))
            .map_err(|_| ProgramError::Custom(ERROR_INVALID_ADDRESS_NETWORK))?;
    }
    Ok(())
}

pub fn get_bitcoin_address(address: &str, network_type: NetworkType) -> Address {
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

#[cfg(test)]
mod tests {
    use arch_program::program_error::ProgramError::Custom;
    use crate::state::*;

    #[test]
    fn test_validate_bitcoin_address() {
        // testnet address valid on testnet
        assert_eq!(
            validate_bitcoin_address(
                "tb1q4sgwdxx8c3l08chkw2w3rewn5armr9urhe0pfk",
                NetworkType::Testnet,
                false
            ),
            Ok(())
        );

        // testnet address not valid in mainnet even if strict is false
        assert_eq!(
            validate_bitcoin_address(
                "tb1q4sgwdxx8c3l08chkw2w3rewn5armr9urhe0pfk",
                NetworkType::Bitcoin,
                false
            ),
            Err(Custom(ERROR_INVALID_ADDRESS_NETWORK))
        );

        // mainnet address valid in testnet if strict checking off
        assert_eq!(
            validate_bitcoin_address(
                "bc1qhz5a7xfh5dj00u32x0j5we6jfpa8vgpqhvaqug",
                NetworkType::Testnet,
                false
            ),
            Ok(())
        );

        // mainnet address fails in testnet if strict is true
        assert_eq!(
            validate_bitcoin_address(
                "bc1qhz5a7xfh5dj00u32x0j5we6jfpa8vgpqhvaqug",
                NetworkType::Testnet,
                true
            ),
            Err(Custom(ERROR_INVALID_ADDRESS_NETWORK))
        );

        // mainnet on mainnet
        assert_eq!(
            validate_bitcoin_address(
                "bc1qhz5a7xfh5dj00u32x0j5we6jfpa8vgpqhvaqug",
                NetworkType::Bitcoin,
                false
            ),
            Ok(())
        );
    }
}
