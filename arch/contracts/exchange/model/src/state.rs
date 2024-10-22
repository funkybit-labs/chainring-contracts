use std::convert::TryInto;
use std::{str, usize};
use arch_program::{
    account::AccountInfo,
    entrypoint,
    pubkey::Pubkey,
    program_error::ProgramError,
};
use crate::error::*;
use crate::serialization::Codable;

pub const VERSION_SIZE: usize = 4;
pub const PUBKEY_SIZE: usize =  32;
pub const PROGRAM_PUBKEY_OFFSET: usize = VERSION_SIZE;
pub const MAX_TOKEN_ID_SIZE: usize = 32;
pub const TOKEN_ID_OFFSET: usize = PROGRAM_PUBKEY_OFFSET + PUBKEY_SIZE;
pub const BALANCE_COUNT_SIZE: usize = 4;
pub const BALANCE_COUNT_OFFSET: usize = TOKEN_ID_OFFSET + MAX_TOKEN_ID_SIZE;
pub const TOKEN_STATE_HEADER_SIZE: usize = VERSION_SIZE + PUBKEY_SIZE + MAX_TOKEN_ID_SIZE + BALANCE_COUNT_SIZE;


pub const MAX_ADDRESS_SIZE: usize = 92;
pub const BALANCE_AMOUNT_SIZE: usize = 8;
pub const BALANCE_AMOUNT_OFFSET: usize = MAX_ADDRESS_SIZE;
pub const BALANCE_SIZE: usize  = MAX_ADDRESS_SIZE + BALANCE_AMOUNT_SIZE;

pub const NETWORK_TYPE_SIZE: usize = 1;
pub const HASH_SIZE: usize = 32;

pub const FEE_ACCOUNT_OFFSET: usize = VERSION_SIZE;
pub const PROGRAM_CHANGE_ADDRESS_OFFSET: usize = FEE_ACCOUNT_OFFSET + MAX_ADDRESS_SIZE;
pub const NETWORK_TYPE_OFFSET: usize = PROGRAM_CHANGE_ADDRESS_OFFSET + MAX_ADDRESS_SIZE;

pub const SETTLEMENT_HASH_OFFSET: usize  = NETWORK_TYPE_OFFSET + NETWORK_TYPE_SIZE;
pub const LAST_SETTLEMENT_HASH_OFFSET: usize = SETTLEMENT_HASH_OFFSET + HASH_SIZE;
pub const LAST_WITHDRAWAL_HASH_OFFSET: usize = LAST_SETTLEMENT_HASH_OFFSET + HASH_SIZE;
pub const FAILED_BALANCE_UPDATES_SIZE_OFFSET: usize = LAST_WITHDRAWAL_HASH_OFFSET + HASH_SIZE;
pub const FAILED_BALANCE_UPDATES_OFFSET: usize = FAILED_BALANCE_UPDATES_SIZE_OFFSET + 1;
pub const ACCOUNT_AND_ADDRESS_INDEX_SIZE: usize = 5;
pub const MAX_FAILED_UPDATES: usize = 100;

pub const FEE_ADDRESS_INDEX: u32 = 0;

pub const EMPTY_HASH: [u8; 32] = [0u8; 32];

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

#[derive(Clone, Debug, PartialEq)]
pub struct AccountAndAddressIndex {
    pub account_index: u8,
    pub address_index: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct TokenState {
    pub version: u32,
    pub program_state_account: Pubkey,
    pub token_id: String,
    pub balances: Vec<Balance>,
}

#[derive(Clone, Debug)]
pub struct ProgramState {
    pub version: u32,
    pub fee_account_address: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
    pub settlement_batch_hash: Hash,
    pub last_settlement_batch_hash: Hash,
    pub last_withdrawal_batch_hash: Hash,
    pub failed_updates: Vec<AccountAndAddressIndex>
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

    pub fn validate_account(accounts: &[AccountInfo], index: u8) -> Result<(), ProgramError> {
        if index as usize >= accounts.len() {
            return Err(ProgramError::Custom(ERROR_INVALID_ACCOUNT_INDEX));
        }
        if TokenState::get_program_state_account_key(&accounts[index as usize])? != *accounts[0].key {
            return Err(ProgramError::Custom(ERROR_PROGRAM_STATE_MISMATCH));
        }
        Ok(())
    }

    pub fn grow_balance_accounts_if_needed(account: &AccountInfo, additional_balances: usize) -> Result<(), ProgramError> {
        let original_data_len = unsafe { account.original_data_len() };

        let num_balances = if original_data_len > 0 {
            TokenState::get_num_balances(account)?
        } else {
            0
        };
        if TOKEN_STATE_HEADER_SIZE + (num_balances + additional_balances) * BALANCE_SIZE > original_data_len {
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
        get_address(account, TOKEN_STATE_HEADER_SIZE + index * BALANCE_SIZE)
    }

    pub fn set_wallet_address(account: &AccountInfo, index: usize, address: &str) -> Result<(), ProgramError> {
        set_string(account, TOKEN_STATE_HEADER_SIZE + index * BALANCE_SIZE, address, MAX_ADDRESS_SIZE)
    }

    pub fn get_wallet_address_last4(account: &AccountInfo, index: usize) -> Result<WalletLast4, ProgramError> {
        Ok(wallet_last4(&Self::get_wallet_address(account, index)?))
    }
}

impl ProgramState {
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
        Ok(data[SETTLEMENT_HASH_OFFSET..SETTLEMENT_HASH_OFFSET+HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn set_last_settlement_hash(account: &AccountInfo, hash: Hash) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[LAST_SETTLEMENT_HASH_OFFSET..LAST_SETTLEMENT_HASH_OFFSET+HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn set_last_withdrawal_hash(account: &AccountInfo, hash: Hash) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[LAST_WITHDRAWAL_HASH_OFFSET..LAST_WITHDRAWAL_HASH_OFFSET+HASH_SIZE].copy_from_slice(
            hash.as_slice()
        ))
    }

    pub fn validate_signer(accounts: &[AccountInfo]) -> Result<(), ProgramError> {
        if !accounts[0].is_signer {
            return Err(ProgramError::Custom(ERROR_INVALID_SIGNER));
        }
        Ok(())
    }

    pub fn clear_failed_updates_count(account: &AccountInfo) -> Result<(), ProgramError> {
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        data[FAILED_BALANCE_UPDATES_SIZE_OFFSET] = 0;
        Ok(())
    }

    pub fn get_failed_updates_count(account: &AccountInfo) -> Result<u8, ProgramError> {
        let data = account.data.try_borrow().map_err(|_| ProgramError::InvalidAccountData)?;
        Ok(data[FAILED_BALANCE_UPDATES_SIZE_OFFSET])
    }

    pub fn push_failed_update(account: &AccountInfo, account_index: u8, address_index: u32) -> Result<(), ProgramError> {
        let current_count = Self::get_failed_updates_count(account)?;
        if current_count as usize == MAX_FAILED_UPDATES {
            return Err(ProgramError::Custom(ERROR_VALUE_TOO_LARGE));
        }
        let offset = FAILED_BALANCE_UPDATES_OFFSET + (current_count as usize * ACCOUNT_AND_ADDRESS_INDEX_SIZE);
        let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
        data[offset] = account_index;
        data[offset+1..offset+ACCOUNT_AND_ADDRESS_INDEX_SIZE].copy_from_slice(
            address_index.to_le_bytes().as_slice()
        );
        data[FAILED_BALANCE_UPDATES_SIZE_OFFSET] = current_count + 1;
        Ok(())
    }

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
        return Err(ProgramError::Custom(ERROR_VALUE_TOO_LARGE));
    }
    let mut data = account.data.try_borrow_mut().map_err(|_| ProgramError::InvalidAccountData)?;
    Ok(data[offset..offset+bytes.len()].copy_from_slice(bytes))
}

fn hash_from_slice(account: &AccountInfo, offset: usize) -> Result<Hash, ProgramError> {
    let mut tmp = EMPTY_HASH;
    tmp[..HASH_SIZE].copy_from_slice(account.data.borrow()[offset..offset+HASH_SIZE]
        .try_into().map_err(|_| ProgramError::InvalidAccountData)?);
    Ok(tmp)
}

pub fn wallet_last4(address: &str) -> WalletLast4 {
    let mut tmp: WalletLast4 = [0u8; 4];
    tmp[0..4].copy_from_slice(&address.as_bytes()[address.len()-4..address.len()]);
    tmp
}

