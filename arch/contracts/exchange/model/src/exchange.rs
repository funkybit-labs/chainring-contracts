use std::convert::TryInto;
use arch_program::{
    account::AccountInfo,
    entrypoint,
    pubkey::Pubkey,
    program_error::ProgramError,
};
use borsh::{BorshDeserialize, BorshSerialize};
use crate::error::*;

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

pub const FEE_ADDRESS_INDEX: u32 = 0;

pub const EMPTY_HASH: [u8; 32] = [0u8; 32];

pub type Hash = [u8; 32];

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

#[derive(Clone)]
pub struct ProgramState {
    pub version: u32,
    pub fee_account_address: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
    pub settlement_batch_hash: Hash,
    pub last_settlement_batch_hash: Hash,
    pub last_withdrawal_batch_hash: Hash,
}

impl TokenState {
    pub fn to_vec(&self) -> Vec<u8> {
        let mut serialized = vec![];
        serialized.extend(self.version.to_le_bytes());
        serialized.extend(self.program_state_account.serialize());
        let mut tmp = [0u8; MAX_TOKEN_ID_SIZE];
        let bytes = self.token_id.as_bytes();
        tmp[..bytes.len()].copy_from_slice(bytes);
        serialized.extend(tmp.as_slice());
        serialized.extend((self.balances.len() as u32).to_le_bytes());
        for balance in self.balances.iter() {
            serialized.extend_from_slice(&balance.to_vec());
        }
        serialized
    }

    pub fn from_slice(data: &[u8]) -> Self {
        let mut tmp = [0u8; MAX_TOKEN_ID_SIZE];
        let mut offset = TOKEN_ID_OFFSET + MAX_TOKEN_ID_SIZE;
        tmp[..MAX_TOKEN_ID_SIZE].copy_from_slice(&data[TOKEN_ID_OFFSET..offset]);
        let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_TOKEN_ID_SIZE);
        let mut token_balances = TokenState {
            version: u32::from_le_bytes(
                data[0..4]
                    .try_into()
                    .expect("slice with incorrect length"),
            ),
            program_state_account: Pubkey::from_slice(
                data[4..36]
                    .try_into()
                    .expect("slice with incorrect length")
            ),
            token_id: String::from_utf8(tmp[..pos].to_vec()).unwrap(),
            balances: vec![],
        };

        let inputs_to_sign_length: usize = u32::from_le_bytes(
            data[offset..offset+4]
                .try_into()
                .expect("slice with incorrect length"),
        ) as usize;

        offset += 4;
        for _ in 0..inputs_to_sign_length {
            token_balances
                .balances
                .push(Balance::from_slice(
                    data[offset..offset + BALANCE_SIZE].try_into().expect("slice with incorrect length")
                ));
            offset += BALANCE_SIZE;
        }
        token_balances
    }

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

    pub fn to_vec(&self) -> Vec<u8> {
        let mut serialized = vec![];
        let mut tmp = [0u8; MAX_ADDRESS_SIZE];
        let bytes = self.address.as_bytes();
        tmp[..bytes.len()].copy_from_slice(bytes);

        serialized.extend(tmp.as_slice());
        serialized.extend(self.balance.to_le_bytes());

        serialized
    }

    pub fn from_slice(data: &[u8]) -> Self {
        let mut tmp = [0u8; MAX_ADDRESS_SIZE];
        tmp[..MAX_ADDRESS_SIZE].copy_from_slice(&data[..MAX_ADDRESS_SIZE]);
        let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_ADDRESS_SIZE);
        Balance {
            address: String::from_utf8(tmp[..pos].to_vec()).unwrap(),
            balance: u64::from_le_bytes(
                data[MAX_ADDRESS_SIZE..MAX_ADDRESS_SIZE+8]
                    .try_into()
                    .expect("slice with incorrect length"),
            )
        }
    }

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

    pub fn from_slice(data: &[u8]) -> Self {
        ProgramState {
            version: u32::from_le_bytes(
                data[0..4]
                    .try_into()
                    .expect("slice with incorrect length"),
            ),
            fee_account_address: Self::address_from_slice(data, 4),
            program_change_address: Self::address_from_slice(data, MAX_ADDRESS_SIZE + 4),
            network_type: NetworkType::from_u8(data[NETWORK_TYPE_OFFSET]),
            settlement_batch_hash: Self::hash_from_slice(data, SETTLEMENT_HASH_OFFSET),
            last_settlement_batch_hash: Self::hash_from_slice(data, LAST_SETTLEMENT_HASH_OFFSET),
            last_withdrawal_batch_hash: Self::hash_from_slice(data, LAST_WITHDRAWAL_HASH_OFFSET)
        }
    }

    fn hash_from_slice(data: &[u8], offset: usize) -> Hash {
        let mut tmp = EMPTY_HASH;
        tmp[..32].copy_from_slice(data[offset..offset+32]
            .try_into()
            .expect("slice with incorrect length"));
        tmp
    }

    fn address_from_slice(data: &[u8], offset: usize) -> String {
        let pos = data[offset..offset+MAX_ADDRESS_SIZE].iter().position(|&r| r == 0).unwrap_or(MAX_TOKEN_ID_SIZE);
        String::from_utf8(data[offset..offset+pos].to_vec()).unwrap()
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

}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct Adjustment {
    pub address_index: u32,
    pub amount: u64,
}

#[derive(Clone, BorshSerialize, BorshDeserialize)]
pub struct TokenStateSetup {
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
    pub token_state_setups: Vec<TokenStateSetup>,
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

fn hash_from_slice(account: &AccountInfo, offset: usize) -> Result<Hash, ProgramError> {
    let mut tmp = EMPTY_HASH;
    tmp[..HASH_SIZE].copy_from_slice(account.data.borrow()[offset..offset+HASH_SIZE]
        .try_into().map_err(|_| ProgramError::InvalidAccountData)?);
    Ok(tmp)
}

