use std::convert::TryInto;
use std::{str, usize};
use std::str::Utf8Error;
use std::io::Write;
use arch_program::{
    account::AccountInfo,
    entrypoint,
    pubkey::Pubkey,
    program_error::ProgramError,
};
use crate::error::*;
use std::io;

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

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return buffer.write(&(match self {
            NetworkType::Bitcoin => 0_u8,
            NetworkType::Testnet => 1_u8,
            NetworkType::Signet => 2_u8,
            NetworkType::Regtest => 3_u8
        }.to_be_bytes()));
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

#[derive(Clone, PartialEq, Debug)]
pub struct AddressIndex {
    pub index: u32,
    pub last4: WalletLast4,
}

impl AddressIndex {
    const SIZE: usize = 8;

    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        return Ok(Self {
            index: u32::from_be_bytes(data[0..4].try_into().map_err(|_| ProgramError::InvalidInstructionData)?),
            last4: data[4..8].try_into().map_err(|_| ProgramError::InvalidInstructionData)?
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return Ok(
            buffer.write(&self.index.to_be_bytes())?
                + buffer.write(&self.last4)?
        )
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Adjustment {
    pub address_index: AddressIndex,
    pub amount: u64,
}

impl Adjustment {
    const SIZE: usize = AddressIndex::SIZE + 8;

    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        return Ok(Self {
            address_index: AddressIndex::from_slice(&data[0..AddressIndex::SIZE])?,
            amount: u64::from_be_bytes(
                data[AddressIndex::SIZE..(AddressIndex::SIZE + 8)]
                    .try_into()
                    .map_err(|_| ProgramError::InvalidInstructionData)?
            )
        })
    }

    pub fn collection_from_slice(data: &[u8]) -> Result<Vec<Self>, ProgramError> {
        let count = usize::from(data[0]);
        let mut adjustments = Vec::with_capacity(count);
        for chunk in data[1..(1 + count * Self::SIZE)].chunks(Self::SIZE) {
            adjustments.push(Self::from_slice(chunk)?);
        }

        return if adjustments.len() == count {
            Ok(adjustments)
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn collection_size(vec: &Vec<Self>) -> usize {
        return 1 + vec.len() * Self::SIZE;
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return Ok(
            self.address_index.write_to_buffer(buffer)?
                + buffer.write(&self.amount.to_be_bytes())?
        )
    }

    pub fn write_collection_to_buffer(adjustments: &Vec<Adjustment>, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = buffer.write(&(adjustments.len() as u8).to_be_bytes())?;
        for adjustment in adjustments {
            bytes_written += adjustment.write_to_buffer(buffer)?;
        }
        return Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct TokenStateSetup {
    pub account_index: u8,
    pub wallet_addresses: Vec<String>,
}

impl TokenStateSetup {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let account_index = data[0];
        let wallets_count = usize::from(data[1]);
        let mut wallet_addresses: Vec<String> = Vec::with_capacity(wallets_count);
        for chunk in data[2..(2 + wallets_count * MAX_ADDRESS_SIZE)].chunks(MAX_ADDRESS_SIZE) {
            wallet_addresses.push(
                string_from_slice(chunk, MAX_ADDRESS_SIZE).map_err(|_| ProgramError::InvalidInstructionData)?
            );
        }

        return Ok(Self {
            account_index: account_index,
            wallet_addresses: wallet_addresses
        })
    }

    pub fn expected_slice_size(&self) -> usize {
        return 2 + self.wallet_addresses.len() * MAX_ADDRESS_SIZE;
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;

        bytes_written += buffer.write(&self.account_index.to_be_bytes())?;
        bytes_written += buffer.write(&(self.wallet_addresses.len() as u8).to_be_bytes())?;

        for address in &self.wallet_addresses {
            bytes_written += write_string_to_buffer(&address, buffer, MAX_ADDRESS_SIZE)?;
        }

        return Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct Withdrawal {
    pub address_index: AddressIndex,
    pub amount: u64,
    pub fee_amount: u64,
}

impl Withdrawal {
    const SIZE: usize = AddressIndex::SIZE + 8 + 8;

    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        return Ok(Self {
            address_index: AddressIndex::from_slice(&data[0..AddressIndex::SIZE])?,
            amount: u64::from_be_bytes(
                data[AddressIndex::SIZE..(AddressIndex::SIZE + 8)].try_into().map_err(|_| ProgramError::InvalidInstructionData)?
            ),
            fee_amount: u64::from_be_bytes(
                data[(AddressIndex::SIZE + 8)..(AddressIndex::SIZE + 16)].try_into().map_err(|_| ProgramError::InvalidInstructionData)?
            ),
        })
    }

    pub fn collection_from_slice(data: &[u8]) -> Result<Vec<Self>, ProgramError> {
        let count = usize::from(data[0]);
        let mut withdrawals = Vec::with_capacity(count);
        for chunk in data[1..(1 + count * Self::SIZE)].chunks(Self::SIZE) {
            withdrawals.push(Self::from_slice(chunk)?);
        }

        return if withdrawals.len() == count {
            Ok(withdrawals)
        } else {
            Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return Ok(
            self.address_index.write_to_buffer(buffer)?
            + buffer.write(&self.amount.to_be_bytes())?
            + buffer.write(&self.fee_amount.to_be_bytes())?
        )
    }

    pub fn write_collection_to_buffer(withdrawals: &Vec<Withdrawal>, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = buffer.write(&(withdrawals.len() as u8).to_be_bytes())?;
        for withdrawal in withdrawals {
            bytes_written += withdrawal.write_to_buffer(buffer)?;
        }
        return Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub enum ProgramInstruction {
    InitProgramState(InitProgramStateParams),
    InitTokenState(InitTokenStateParams),
    InitWalletBalances(InitWalletBalancesParams),
    BatchDeposit(DepositBatchParams),
    BatchWithdraw(WithdrawBatchParams),
    PrepareBatchSettlement(SettlementBatchParams),
    SubmitBatchSettlement(SettlementBatchParams),
    RollbackBatchSettlement(),
    RollbackBatchWithdraw(RollbackWithdrawBatchParams),
}

impl ProgramInstruction {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let instr_type = data[0];
        let params_bytes = &data[1..];
        match instr_type {
            0 => Ok(Self::InitProgramState(InitProgramStateParams::from_slice(params_bytes)?)),
            1 => Ok(Self::InitTokenState(InitTokenStateParams::from_slice(params_bytes)?)),
            2 => Ok(Self::InitWalletBalances(InitWalletBalancesParams::from_slice(params_bytes)?)),
            3 => Ok(Self::BatchDeposit(DepositBatchParams::from_slice(params_bytes)?)),
            4 => Ok(Self::BatchWithdraw(WithdrawBatchParams::from_slice(params_bytes)?)),
            5 => Ok(Self::PrepareBatchSettlement(SettlementBatchParams::from_slice(params_bytes)?)),
            6 => Ok(Self::SubmitBatchSettlement(SettlementBatchParams::from_slice(params_bytes)?)),
            7 => Ok(Self::RollbackBatchSettlement()),
            8 => Ok(Self::RollbackBatchWithdraw(RollbackWithdrawBatchParams::from_slice(params_bytes)?)),
            _ => Err(ProgramError::InvalidInstructionData)
        }
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        return match self {
            Self::InitProgramState(params) => {
                let _ = buffer.write(&0_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::InitTokenState(params) => {
                let _ = buffer.write(&1_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::InitWalletBalances(params) => {
                let _ = buffer.write(&2_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::BatchDeposit(params) => {
                let _ = buffer.write(&3_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::BatchWithdraw(params) => {
                let _ = buffer.write(&4_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::PrepareBatchSettlement(params) => {
                let _ = buffer.write(&5_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::SubmitBatchSettlement(params) => {
                let _ = buffer.write(&6_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
            Self::RollbackBatchSettlement() => {
                let _ = buffer.write(&7_u8.to_be_bytes())?;
                Ok(buffer)
            }
            Self::RollbackBatchWithdraw(params) => {
                let _ = buffer.write(&8_u8.to_be_bytes())?;
                let _ = params.write_to_buffer(&mut buffer)?;
                Ok(buffer)
            }
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct InitProgramStateParams {
    pub fee_account: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
}

impl InitProgramStateParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let mut offset = 0;

        let fee_account = string_from_slice(&data[offset..], MAX_ADDRESS_SIZE)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        offset += MAX_ADDRESS_SIZE;

        let program_change_address = string_from_slice(&data[offset..], MAX_ADDRESS_SIZE)
            .map_err(|_| ProgramError::InvalidInstructionData)?;
        offset += MAX_ADDRESS_SIZE;

        let network_type = NetworkType::from_u8(data[offset]);

        return Ok(InitProgramStateParams {
            fee_account: fee_account,
            program_change_address: program_change_address,
            network_type: network_type
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;

        bytes_written += write_string_to_buffer(&self.fee_account, buffer, MAX_ADDRESS_SIZE)?;
        bytes_written += write_string_to_buffer(&self.program_change_address, buffer, MAX_ADDRESS_SIZE)?;
        bytes_written += self.network_type.write_to_buffer(buffer)?;

        return Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct InitTokenStateParams {
    pub token_id: String,
}

impl InitTokenStateParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        return Ok(Self {
            token_id: string_from_slice(&data, MAX_TOKEN_ID_SIZE)
                .map_err(|_| ProgramError::InvalidInstructionData)
                ?.to_string()
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return write_string_to_buffer(&self.token_id, buffer, MAX_TOKEN_ID_SIZE)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct InitWalletBalancesParams {
    pub token_state_setups: Vec<TokenStateSetup>,
}

impl InitWalletBalancesParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let token_state_setups_count = usize::from(data[0]);
        let mut token_state_setups: Vec<TokenStateSetup> = Vec::with_capacity(token_state_setups_count);
        let mut offset = 1;

        while offset < data.len() {
            let token_state_setup = TokenStateSetup::from_slice(&data[offset..])?;
            offset += token_state_setup.expected_slice_size();
            token_state_setups.push(token_state_setup);
        }

        if token_state_setups.len() != token_state_setups_count {
            return Err(ProgramError::InvalidInstructionData);
        }

        return Ok(Self {
            token_state_setups: token_state_setups
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;
        bytes_written += buffer.write(&(self.token_state_setups.len() as u8).to_be_bytes())?;

        for token_state_setup in &self.token_state_setups {
            bytes_written += token_state_setup.write_to_buffer(buffer)?;
        }

        return Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct DepositBatchParams {
    pub token_deposits: Vec<TokenDeposits>,
}

impl DepositBatchParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let token_deposits_count = usize::from(data[0]);
        let mut token_deposits: Vec<TokenDeposits> = Vec::with_capacity(token_deposits_count);
        let mut offset = 1;

        while offset < data.len() {
            let deposits = TokenDeposits::from_slice(&data[offset..])?;
            offset += deposits.expected_slice_size();
            token_deposits.push(deposits);
        }

        if token_deposits.len() != token_deposits_count {
            return Err(ProgramError::InvalidInstructionData);
        }

        return Ok(Self {
            token_deposits: token_deposits
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = buffer.write(&(self.token_deposits.len() as u8).to_be_bytes())?;
        for token_deposits in &self.token_deposits {
            bytes_written += token_deposits.write_to_buffer(buffer)?;
        }
        Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct TokenDeposits {
    pub account_index: u8,
    pub deposits: Vec<Adjustment>,
}

impl TokenDeposits {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        return Ok(Self {
            account_index: data[0],
            deposits: Adjustment::collection_from_slice(&data[1..])?
        })
    }

    pub fn expected_slice_size(&self) -> usize {
        return 2 + self.deposits.len() * Adjustment::SIZE;
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;
        bytes_written += buffer.write(&self.account_index.to_be_bytes())?;
        bytes_written += Adjustment::write_collection_to_buffer(&self.deposits, buffer)?;
        return Ok(bytes_written)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct TokenWithdrawals {
    pub account_index: u8,
    pub withdrawals: Vec<Withdrawal>,
}

impl TokenWithdrawals {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        return Ok(Self {
            account_index: data[0],
            withdrawals: Withdrawal::collection_from_slice(&data[1..])?
        })
    }

    pub fn expected_slice_size(&self) -> usize {
        return 2 + self.withdrawals.len() * Withdrawal::SIZE;
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return Ok(buffer.write(&self.account_index.to_be_bytes())? + Withdrawal::write_collection_to_buffer(&self.withdrawals, buffer)?)
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct WithdrawBatchParams {
    pub tx_hex: Vec<u8>,
    pub change_amount: u64,
    pub token_withdrawals: Vec<TokenWithdrawals>,
}

impl WithdrawBatchParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let mut offset = 0;

        let tx_hex_size = usize::from(u16::from_be_bytes(data[0..2].try_into().unwrap()));
        offset += 2;
        let tx_hex = data[offset..(offset + tx_hex_size)].to_vec();
        offset += tx_hex_size;

        let change_amount = u64::from_be_bytes(data[offset..(offset + 8)].try_into().unwrap());
        offset += 8;

        let token_withdrawals_count = usize::from(data[offset]);
        offset += 1;

        let mut token_withdrawals: Vec<TokenWithdrawals> = Vec::with_capacity(token_withdrawals_count);
        while offset < data.len() {
            let withdrawals = TokenWithdrawals::from_slice(&data[offset..])?;
            offset += withdrawals.expected_slice_size();
            token_withdrawals.push(withdrawals);
        }

        if token_withdrawals.len() != token_withdrawals_count {
            return Err(ProgramError::InvalidInstructionData)
        }

        return Ok(Self {
            tx_hex: tx_hex,
            change_amount: change_amount,
            token_withdrawals: token_withdrawals,
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;
        bytes_written += buffer.write(&(self.tx_hex.len() as u16).to_be_bytes())?;
        bytes_written += buffer.write(self.tx_hex.as_slice())?;
        bytes_written += buffer.write(&self.change_amount.to_be_bytes())?;
        bytes_written += buffer.write(&((self.token_withdrawals.len() as u8).to_be_bytes()))?;
        for token_withdrawals in &self.token_withdrawals {
            bytes_written += token_withdrawals.write_to_buffer(buffer)?;
        }
        return Ok(bytes_written)
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        let _ = self.write_to_buffer(&mut buffer)?;
        return Ok(buffer);
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct SettlementAdjustments {
    pub account_index: u8,
    pub increments: Vec<Adjustment>,
    pub decrements: Vec<Adjustment>,
    pub fee_amount: u64,
}

impl SettlementAdjustments {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let account_index = data[0];
        let mut offset: usize = 1;

        let increments = Adjustment::collection_from_slice(&data[offset..])?;
        offset += Adjustment::collection_size(&increments);

        let decrements = Adjustment::collection_from_slice(&data[offset..])?;
        offset += Adjustment::collection_size(&decrements);

        return Ok(Self {
            account_index: account_index,
            increments: increments,
            decrements: decrements,
            fee_amount: u64::from_be_bytes(data[offset..(offset + 8)].try_into().unwrap())
        })
    }

    pub fn expected_slice_size(&self) -> usize {
        return 1 + Adjustment::collection_size(&self.increments) + Adjustment::collection_size(&self.decrements) + 8;
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        return Ok(
            buffer.write(&self.account_index.to_be_bytes())?
                + Adjustment::write_collection_to_buffer(&self.increments, buffer)?
                + Adjustment::write_collection_to_buffer(&self.decrements, buffer)?
                + buffer.write(&self.fee_amount.to_be_bytes())?
        )
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct SettlementBatchParams {
    pub settlements: Vec<SettlementAdjustments>
}

impl SettlementBatchParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let settlements_count = usize::from(u16::from_be_bytes(data[0..2].try_into().map_err(|_| ProgramError::InvalidInstructionData)?));
        let mut settlements: Vec<SettlementAdjustments> = Vec::with_capacity(settlements_count);

        let mut offset: usize = 2;

        while offset < data.len() {
            let settlement_adjustments = SettlementAdjustments::from_slice(&data[offset..])?;
            offset += settlement_adjustments.expected_slice_size();
            settlements.push(settlement_adjustments);
        }

        if settlements.len() != settlements_count {
            return Err(ProgramError::InvalidInstructionData)
        }

        return Ok(Self {
            settlements: settlements
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;
        bytes_written += buffer.write(&((self.settlements.len() as u16).to_be_bytes()))?;
        for settlement_adjustments in &self.settlements {
            bytes_written += settlement_adjustments.write_to_buffer(buffer)?;
        }
        return Ok(bytes_written)
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        let _ = self.write_to_buffer(&mut buffer)?;
        return Ok(buffer);
    }
}

#[derive(Clone, PartialEq, Debug)]
pub struct RollbackWithdrawBatchParams {
    pub token_withdrawals: Vec<TokenWithdrawals>,
}

impl RollbackWithdrawBatchParams {
    pub fn from_slice(data: &[u8]) -> Result<Self, ProgramError> {
        let mut offset = 0;

        let token_withdrawals_count = usize::from(data[offset]);
        offset += 1;

        let mut token_withdrawals: Vec<TokenWithdrawals> = Vec::with_capacity(token_withdrawals_count);
        while offset < data.len() {
            let withdrawals = TokenWithdrawals::from_slice(&data[offset..])?;
            offset += withdrawals.expected_slice_size();
            token_withdrawals.push(withdrawals);
        }

        if token_withdrawals.len() != token_withdrawals_count {
            return Err(ProgramError::InvalidInstructionData)
        }

        return Ok(Self {
            token_withdrawals: token_withdrawals,
        })
    }

    pub fn write_to_buffer(&self, buffer: &mut Vec<u8>) -> Result<usize, io::Error> {
        let mut bytes_written = 0;
        bytes_written += buffer.write(&((self.token_withdrawals.len() as u8).to_be_bytes()))?;
        for token_withdrawals in &self.token_withdrawals {
            bytes_written += token_withdrawals.write_to_buffer(buffer)?;
        }
        return Ok(bytes_written)
    }

    pub fn to_vec(&self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        let _ = self.write_to_buffer(&mut buffer)?;
        return Ok(buffer);
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

fn string_from_slice(data: &[u8], max_size: usize) -> Result<String, Utf8Error> {
    let end_pos = data[0..max_size].iter().position(|&r| r == 0).unwrap_or(MAX_ADDRESS_SIZE);
    return Ok(str::from_utf8(&data[..end_pos])?.to_string())
}

fn write_string_to_buffer(string: &String, buffer: &mut Vec<u8>, padded_size: usize) -> Result<usize, io::Error> {
    let mut bytes_written = 0;
    let str_bytes = string.as_bytes();
    let padding = padded_size - str_bytes.len();

    bytes_written += buffer.write(str_bytes)?;
    buffer.resize(buffer.len() + padding, 0);
    bytes_written += padding;

    Ok(bytes_written)
}

pub fn wallet_last4(address: &str) -> WalletLast4 {
    let mut tmp: WalletLast4 = [0u8; 4];
    tmp[0..4].copy_from_slice(&address.as_bytes()[address.len()-4..address.len()]);
    tmp
}

/// Running Tests
///
#[cfg(test)]
mod tests {
    use crate::exchange::*;

    #[test]
    fn test_instructions_serialization() {
        let instruction = ProgramInstruction::InitProgramState(InitProgramStateParams {
            fee_account: "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string(),
            program_change_address: "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_string(),
            network_type: NetworkType::Regtest
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::InitTokenState(InitTokenStateParams {
            token_id: "BTC".to_string(),
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::InitWalletBalances(InitWalletBalancesParams {
            token_state_setups: vec![
                TokenStateSetup {
                    account_index: 0,
                    wallet_addresses: vec![
                        "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string(),
                        "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_string()
                    ]
                },
                TokenStateSetup {
                    account_index: 1,
                    wallet_addresses: vec![
                        "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_string(),
                        "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string()
                    ]
                }
            ]
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::BatchDeposit(DepositBatchParams {
            token_deposits: vec![
                TokenDeposits {
                    account_index: 0,
                    deposits: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 456
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 321,
                                last4: [5, 6, 7, 8]
                            },
                            amount: 654
                        }
                    ]
                },
                TokenDeposits {
                    account_index: 1,
                    deposits: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 222
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 333,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 444
                        }
                    ]
                }
            ]
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::BatchWithdraw(WithdrawBatchParams {
            tx_hex: vec![1, 2, 3],
            change_amount: 123,
            token_withdrawals: vec![
                TokenWithdrawals {
                    account_index: 0,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 456,
                            fee_amount: 789
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 321,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 654,
                            fee_amount: 987
                        }
                    ]
                },
                TokenWithdrawals {
                    account_index: 1,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 222,
                            fee_amount: 333
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 444,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 555,
                            fee_amount: 666
                        }
                    ]
                }
            ]
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::PrepareBatchSettlement(SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 0,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 222
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 333,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 444
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 555,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 666
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 777,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 888
                        }
                    ],
                    fee_amount: 123
                },
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 1111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 2222
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 3333,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 4444
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 5555,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 6666
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 7777,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 8888
                        }
                    ],
                    fee_amount: 1234
                }
            ]
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::SubmitBatchSettlement(SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 0,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 222
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 333,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 444
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 555,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 666
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 777,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 888
                        }
                    ],
                    fee_amount: 123
                },
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 1111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 2222
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 3333,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 4444
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 5555,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 6666
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 7777,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 8888
                        }
                    ],
                    fee_amount: 1234
                }
            ]
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::RollbackBatchWithdraw(RollbackWithdrawBatchParams {
            token_withdrawals: vec![
                TokenWithdrawals {
                    account_index: 0,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 456,
                            fee_amount: 789
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 321,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 654,
                            fee_amount: 987
                        }
                    ]
                },
                TokenWithdrawals {
                    account_index: 1,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4]
                            },
                            amount: 222,
                            fee_amount: 333
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 444,
                                last4: [4, 3, 2, 1]
                            },
                            amount: 555,
                            fee_amount: 666
                        }
                    ]
                }
            ]
        });
        assert_eq!(instruction, ProgramInstruction::from_slice(&instruction.to_vec().unwrap()).unwrap());
    }
}

