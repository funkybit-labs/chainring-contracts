use std::io;
use std::io::{Cursor, Error, Read, Write};
use arch_program::pubkey::Pubkey;
use crate::state::{AccountType, Balance, Event, EVENT_SIZE, Hash, MAX_ADDRESS_SIZE, MAX_TOKEN_ID_SIZE, NetworkType, ProgramState, RuneReceiverState, TokenState, WithdrawState};
use crate::instructions::*;

pub trait ReadExt: io::Read {
    fn read_u8(&mut self) -> Result<u8, io::Error>;
    fn read_u16(&mut self) -> Result<u16, io::Error>;
    fn read_u16_as_usize(&mut self) -> Result<usize, io::Error>;
    fn read_u32(&mut self) -> Result<u32, io::Error>;
    fn read_u32_as_usize(&mut self) -> Result<usize, io::Error>;
    fn read_u64(&mut self) -> Result<u64, io::Error>;
    fn read_string(&mut self) -> Result<String, io::Error>;
    fn read_string_with_padding(&mut self, size: usize) -> Result<String, io::Error>;
    fn read_pubkey(&mut self) -> Result<Pubkey, io::Error>;
    fn read_hash(&mut self) -> Result<Hash, io::Error>;
}

impl<R: io::Read + ?Sized> ReadExt for R {
    fn read_u8(&mut self) -> Result<u8, io::Error> {
        let mut val = [0; 1];
        self.read_exact(&mut val)?;
        Ok(val[0])
    }

    fn read_u16_as_usize(&mut self) -> Result<usize, io::Error> {
        Ok(usize::from(self.read_u16()?))
    }

    fn read_u16(&mut self) -> Result<u16, io::Error> {
        let mut val = [0; 2];
        self.read_exact(&mut val[..])?;
        Ok(u16::from_le_bytes(val))
    }

    fn read_u32(&mut self) -> Result<u32, io::Error> {
        let mut val = [0; 4];
        self.read_exact(&mut val[..])?;
        Ok(u32::from_le_bytes(val))
    }

    fn read_u32_as_usize(&mut self) -> Result<usize, io::Error> {
        Ok(self.read_u32()? as usize)
    }

    fn read_u64(&mut self) -> Result<u64, io::Error> {
        let mut val = [0; 8];
        self.read_exact(&mut val[..])?;
        Ok(u64::from_le_bytes(val))
    }

    fn read_string(&mut self) -> Result<String, io::Error> {
        let mut str = String::new();
        let str_size = self.read_u16()?;
        self.take(str_size as u64).read_to_string(&mut str)?;
        Ok(str)
    }

    fn read_string_with_padding(&mut self, size: usize) -> Result<String, io::Error> {
        let mut vec = Vec::with_capacity(size);
        self.take(size as u64).read_to_end(&mut vec)?;
        let end_pos = vec.iter().position(|&b| b == 0).unwrap_or(size);
        vec.truncate(end_pos);
        String::from_utf8(vec)
            .map_err(|_| io::Error::new(io::ErrorKind::Other, "Invalid UTF8 string"))
    }

    fn read_pubkey(&mut self) -> Result<Pubkey, io::Error> {
        let mut bytes = [0; 32];
        self.read_exact(&mut bytes[..])?;
        Ok(Pubkey::from(bytes))
    }

    fn read_hash(&mut self) -> Result<Hash, io::Error> {
        let mut bytes = [0; 32];
        self.read_exact(&mut bytes[..])?;
        Ok(bytes)
    }
}

pub trait WriteExt: io::Write {
    fn write_u8(&mut self, v: u8) -> Result<usize, io::Error>;
    fn write_u16(&mut self, v: u16) -> Result<usize, io::Error>;
    fn write_usize_as_u16(&mut self, v: usize) -> Result<usize, io::Error>;
    fn write_u32(&mut self, v: u32) -> Result<usize, io::Error>;
    fn write_usize_as_u32(&mut self, v: usize) -> Result<usize, io::Error>;
    fn write_u64(&mut self, v: u64) -> Result<usize, io::Error>;
    fn write_string(&mut self, v: &String) -> Result<usize, io::Error>;
    fn write_string_with_padding(&mut self, v: &String, size: usize) -> Result<usize, io::Error>;
    fn write_padding(&mut self, padding_len: usize) -> Result<usize, io::Error>;
    fn write_pubkey(&mut self, v: &Pubkey) -> Result<usize, io::Error>;
    fn write_hash(&mut self, v: &Hash) -> Result<usize, io::Error>;
}

impl<W: io::Write> WriteExt for W {
    fn write_u8(&mut self, v: u8) -> Result<usize, io::Error> {
        _ = self.write_all(&[v])?;
        Ok(1)
    }

    fn write_u16(&mut self, v: u16) -> Result<usize, io::Error> {
        let bytes = v.to_le_bytes();
        _ = self.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_usize_as_u16(&mut self, v: usize) -> Result<usize, io::Error> {
        let bytes = (v as u16).to_le_bytes();
        _ = self.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_u32(&mut self, v: u32) -> Result<usize, io::Error> {
        let bytes = v.to_le_bytes();
        _ = self.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_usize_as_u32(&mut self, v: usize) -> Result<usize, io::Error> {
        let bytes = (v as u32).to_le_bytes();
        _ = self.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_u64(&mut self, v: u64) -> Result<usize, io::Error> {
        let bytes = v.to_le_bytes();
        _ = self.write_all(&bytes)?;
        Ok(bytes.len())
    }

    fn write_string(&mut self, v: &String) -> Result<usize, io::Error> {
        let bytes = v.as_bytes();
        let mut bytes_written = self.write_usize_as_u16(bytes.len())?;
        self.write_all(&bytes)?;
        bytes_written += bytes.len();
        Ok(bytes_written)
    }

    fn write_string_with_padding(&mut self, v: &String, size: usize) -> Result<usize, io::Error> {
        let bytes = v.as_bytes();
        self.write_all(&bytes)?;
        Ok(bytes.len() + self.write_padding(size - bytes.len())?)
    }

    fn write_padding(&mut self, padding_len: usize) -> Result<usize, io::Error> {
        Ok(
            if padding_len > 0 {
                let padding = vec![0; padding_len];
                self.write_all(&padding)?;
                padding_len
            } else {
                0
            }
        )
    }

    fn write_pubkey(&mut self, v: &Pubkey) -> Result<usize, io::Error> {
        self.write_all(&v.0)?;
        Ok(v.0.len())
    }

    fn write_hash(&mut self, v: &Hash) -> Result<usize, io::Error> {
        self.write_all(v)?;
        Ok(v.len())
    }
}

pub trait Codable: Sized {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error>;

    fn decode_from_slice(data: &[u8]) -> Result<Self, io::Error> {
        let mut reader = Cursor::new(data);
        Self::decode(&mut reader)
    }

    fn encode<W: io::Write + ?Sized>(&self, writer: &mut W) -> Result<usize, io::Error>;

    fn encode_to_vec(&self) -> Result<Vec<u8>, io::Error> {
        let mut buffer = Vec::new();
        _ = self.encode(&mut buffer)?;
        Ok(buffer)
    }
}

impl ProgramInstruction {
    pub fn params_raw_data(instruction_data: &[u8]) -> &[u8] {
        &instruction_data[1..]
    }
}

impl Codable for ProgramInstruction {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let instr_type = reader.read_u8()?;
        match instr_type {
            0 => Ok(Self::InitProgramState(InitProgramStateParams::decode(reader)?)),
            1 => Ok(Self::InitTokenState(InitTokenStateParams::decode(reader)?)),
            2 => Ok(Self::InitWalletBalances(InitWalletBalancesParams::decode(reader)?)),
            3 => Ok(Self::BatchDeposit(DepositBatchParams::decode(reader)?)),
            4 => Ok(Self::PrepareBatchWithdraw(WithdrawBatchParams::decode(reader)?)),
            5 => Ok(Self::PrepareBatchSettlement(SettlementBatchParams::decode(reader)?)),
            6 => Ok(Self::SubmitBatchSettlement(SettlementBatchParams::decode(reader)?)),
            7 => Ok(Self::RollbackBatchSettlement()),
            8 => Ok(Self::RollbackBatchWithdraw(RollbackWithdrawBatchParams::decode(reader)?)),
            9 => Ok(Self::SubmitBatchWithdraw(WithdrawBatchParams::decode(reader)?)),
            10 => Ok(Self::InitRuneReceiverState()),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Invalid instruction type"))
        }
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        match self {
            Self::InitProgramState(params) => {
                Ok(writer.write_u8(0)? + params.encode(&mut writer)?)
            }
            Self::InitTokenState(params) => {
                Ok(writer.write_u8(1)? + params.encode(&mut writer)?)
            }
            Self::InitWalletBalances(params) => {
                Ok(writer.write_u8(2)? + params.encode(&mut writer)?)
            }
            Self::BatchDeposit(params) => {
                Ok(writer.write_u8(3)? + params.encode(&mut writer)?)
            }
            Self::PrepareBatchWithdraw(params) => {
                Ok(writer.write_u8(4)? + params.encode(&mut writer)?)
            }
            Self::PrepareBatchSettlement(params) => {
                Ok(writer.write_u8(5)? + params.encode(&mut writer)?)
            }
            Self::SubmitBatchSettlement(params) => {
                Ok(writer.write_u8(6)? + params.encode(&mut writer)?)
            }
            Self::RollbackBatchSettlement() => {
                Ok(writer.write_u8(7)?)
            }
            Self::RollbackBatchWithdraw(params) => {
                Ok(writer.write_u8(8)? + params.encode(&mut writer)?)
            }
            Self::SubmitBatchWithdraw(params) => {
                Ok(writer.write_u8(9)? + params.encode(&mut writer)?)
            }
            Self::InitRuneReceiverState() => {
                Ok(writer.write_u8(10)?)
            }
        }
    }
}

impl Codable for NetworkType {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(match reader.read_u8()? {
            0 => Self::Bitcoin,
            1 => Self::Testnet,
            2 => Self::Signet,
            3 => Self::Regtest,
            _ => Self::Bitcoin
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(writer.write_u8(match self {
            Self::Bitcoin => 0,
            Self::Testnet => 1,
            Self::Signet => 2,
            Self::Regtest => 3
        })?)
    }
}

impl Codable for InputUtxoType {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(match reader.read_u8()? {
            0 => Self::Bitcoin,
            1 => Self::Rune,
            _ => Self::Bitcoin
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(writer.write_u8(match self {
            Self::Bitcoin => 0,
            Self::Rune => 1,
        })?)
    }
}

impl Codable for TokenStateSetup {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let account_index = reader.read_u8()?;
        let wallets_count = reader.read_u16_as_usize()?;
        let mut wallet_addresses: Vec<String> = Vec::with_capacity(wallets_count);
        for _ in 0..wallets_count {
            wallet_addresses.push(
                reader.read_string()?
            );
        }

        Ok(Self {
            account_index,
            wallet_addresses,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_u8(self.account_index)?;
        bytes_written += writer.write_usize_as_u16(self.wallet_addresses.len())?;

        for wallet_address in &self.wallet_addresses {
            bytes_written += writer.write_string(&wallet_address)?
        }

        Ok(bytes_written)
    }
}

impl Codable for AddressIndex {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let index = reader.read_u32()?;
        let mut last4 = [0; 4];
        reader.read_exact(&mut last4)?;

        Ok(Self {
            index,
            last4,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_u32(self.index)?;
        writer.write_all(&self.last4)?;
        bytes_written += self.last4.len();
        Ok(bytes_written)
    }
}

impl Codable for Adjustment {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            address_index: AddressIndex::decode(reader)?,
            amount: reader.read_u64()?,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(
            self.address_index.encode(writer)? + writer.write_u64(self.amount)?
        )
    }
}

impl Codable for Withdrawal {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            address_index: AddressIndex::decode(reader)?,
            amount: reader.read_u64()?,
            fee_address_index: AddressIndex::decode(reader)?,
            fee_amount: reader.read_u64()?,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(
            self.address_index.encode(writer)?
                + writer.write_u64(self.amount)?
                + self.fee_address_index.encode(writer)?
                + writer.write_u64(self.fee_amount)?
        )
    }
}

impl Codable for TokenDeposits {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let account_index = reader.read_u8()?;

        let deposits_count = reader.read_u16_as_usize()?;
        let mut deposits = Vec::with_capacity(deposits_count);
        for _ in 0..deposits_count {
            deposits.push(Adjustment::decode(reader)?);
        }

        Ok(Self {
            account_index,
            deposits,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_u8(self.account_index)?;
        bytes_written += writer.write_usize_as_u16(self.deposits.len())?;
        for deposit in &self.deposits {
            bytes_written += deposit.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for TokenWithdrawals {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let account_index = reader.read_u8()?;
        let fee_account_index = reader.read_u8()?;

        let withdrawals_count = reader.read_u16_as_usize()?;
        let mut withdrawals = Vec::with_capacity(withdrawals_count);
        for _ in 0..withdrawals_count {
            withdrawals.push(Withdrawal::decode(reader)?);
        }

        Ok(Self {
            account_index,
            fee_account_index,
            withdrawals,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, Error> {
        let mut bytes_written = writer.write_u8(self.account_index)?;
        bytes_written += writer.write_u8(self.fee_account_index)?;

        bytes_written += writer.write_usize_as_u16(self.withdrawals.len())?;
        for withdrawal in &self.withdrawals {
            bytes_written += withdrawal.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for SettlementAdjustments {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let account_index = reader.read_u8()?;

        let increments_count = reader.read_u16_as_usize()?;
        let mut increments = Vec::with_capacity(increments_count);
        for _ in 0..increments_count {
            increments.push(Adjustment::decode(reader)?);
        }

        let decrements_count = reader.read_u16_as_usize()?;
        let mut decrements = Vec::with_capacity(decrements_count);
        for _ in 0..decrements_count {
            decrements.push(Adjustment::decode(reader)?);
        }

        let fee_amount = reader.read_u64()?;

        Ok(Self {
            account_index,
            increments,
            decrements,
            fee_amount,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_u8(self.account_index)?;

        bytes_written += writer.write_usize_as_u16(self.increments.len())?;
        for increment in &self.increments {
            bytes_written += increment.encode(writer)?;
        }

        bytes_written += writer.write_usize_as_u16(self.decrements.len())?;
        for decrement in &self.decrements {
            bytes_written += decrement.encode(writer)?;
        }

        bytes_written += writer.write_u64(self.fee_amount)?;

        Ok(bytes_written)
    }
}

impl Codable for InitProgramStateParams {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            fee_account: reader.read_string()?,
            program_change_address: reader.read_string()?,
            network_type: NetworkType::decode(reader)?,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(
            writer.write_string(&self.fee_account)? +
                writer.write_string(&self.program_change_address)? +
                self.network_type.encode(writer)?
        )
    }
}

impl Codable for InitTokenStateParams {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            token_id: reader.read_string()?
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        writer.write_string(&self.token_id)
    }
}

impl Codable for InitWalletBalancesParams {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let token_state_setups_count = reader.read_u16_as_usize()?;

        let mut token_state_setups: Vec<TokenStateSetup> = Vec::with_capacity(token_state_setups_count);
        for _ in 0..token_state_setups_count {
            token_state_setups.push(TokenStateSetup::decode(reader)?);
        }

        Ok(Self {
            token_state_setups
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_usize_as_u16(self.token_state_setups.len())?;
        for token_state_setups in &self.token_state_setups {
            bytes_written += token_state_setups.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for DepositBatchParams {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let token_deposits_count = reader.read_u16_as_usize()?;

        let mut token_deposits = Vec::with_capacity(token_deposits_count);
        for _ in 0..token_deposits_count {
            token_deposits.push(TokenDeposits::decode(reader)?);
        }

        Ok(Self {
            token_deposits
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_usize_as_u16(self.token_deposits.len())?;
        for token_deposits in &self.token_deposits {
            bytes_written += token_deposits.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for WithdrawBatchParams {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let tx_hex_size = reader.read_u16()?;
        let mut tx_hex = Vec::with_capacity(usize::from(tx_hex_size));
        reader.take(tx_hex_size as u64).read_to_end(&mut tx_hex)?;

        let change_amount = reader.read_u64()?;

        let token_withdrawals_count = reader.read_u16_as_usize()?;
        let mut token_withdrawals = Vec::with_capacity(token_withdrawals_count);
        for _ in 0..token_withdrawals_count {
            token_withdrawals.push(TokenWithdrawals::decode(reader)?);
        }

        let input_utxo_type_count = reader.read_u16_as_usize()?;
        let mut input_utxo_types = Vec::with_capacity(input_utxo_type_count);
        for _ in 0..input_utxo_type_count {
            input_utxo_types.push(InputUtxoType::decode(reader)?);
        }

        Ok(Self {
            tx_hex,
            change_amount,
            input_utxo_types,
            token_withdrawals,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_usize_as_u16(self.tx_hex.len())?;
        writer.write_all(self.tx_hex.as_slice())?;
        bytes_written += self.tx_hex.len();
        bytes_written += writer.write_u64(self.change_amount)?;
        bytes_written += writer.write_usize_as_u16(self.token_withdrawals.len())?;
        for token_withdrawals in &self.token_withdrawals {
            bytes_written += token_withdrawals.encode(writer)?;
        }
        bytes_written += writer.write_usize_as_u16(self.input_utxo_types.len())?;
        for input_utxo_type in &self.input_utxo_types {
            bytes_written += input_utxo_type.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for SettlementBatchParams {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let settlements_count = reader.read_u16_as_usize()?;

        let mut settlements = Vec::with_capacity(settlements_count);
        for _ in 0..settlements_count {
            settlements.push(SettlementAdjustments::decode(reader)?);
        }

        Ok(Self {
            settlements
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_usize_as_u16(self.settlements.len())?;
        for settlement in &self.settlements {
            bytes_written += settlement.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for RollbackWithdrawBatchParams {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let token_withdrawals_count = reader.read_u16_as_usize()?;
        let mut token_withdrawals = Vec::with_capacity(token_withdrawals_count);
        for _ in 0..token_withdrawals_count {
            token_withdrawals.push(TokenWithdrawals::decode(reader)?);
        }
        Ok(Self {
            token_withdrawals
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let mut bytes_written = writer.write_usize_as_u16(self.token_withdrawals.len())?;
        for token_withdrawals in &self.token_withdrawals {
            bytes_written += token_withdrawals.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for Balance {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(Self {
            address: reader.read_string_with_padding(MAX_ADDRESS_SIZE)?,
            balance: reader.read_u64()?,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(
            writer.write_string_with_padding(&self.address, MAX_ADDRESS_SIZE)? +
                writer.write_u64(self.balance)?
        )
    }
}

impl Codable for Event {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        let mut event_data = Vec::with_capacity(EVENT_SIZE);
        reader.take(EVENT_SIZE as u64).read_to_end(&mut event_data)?;

        let mut event_data_reader = Cursor::new(event_data);
        let event_type = event_data_reader.read_u8()?;
        match event_type {
            0 => Ok(Self::FailedSettlement {
                account_index: event_data_reader.read_u8()?,
                address_index: event_data_reader.read_u32()?,
                requested_amount: event_data_reader.read_u64()?,
                balance: event_data_reader.read_u64()?,
                error_code: event_data_reader.read_u32()?
            }),
            1 => Ok(Self::FailedWithdrawal {
                account_index: event_data_reader.read_u8()?,
                address_index: event_data_reader.read_u32()?,
                fee_account_index: event_data_reader.read_u8()?,
                fee_address_index: event_data_reader.read_u32()?,
                requested_amount: event_data_reader.read_u64()?,
                fee_amount: event_data_reader.read_u64()?,
                balance: event_data_reader.read_u64()?,
                balance_in_fee_token: event_data_reader.read_u64()?,
                error_code: event_data_reader.read_u32()?
            }),
            _ => Err(io::Error::new(io::ErrorKind::Other, "Invalid event type"))
        }
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        let bytes_written = match self {
            Self::FailedSettlement { account_index, address_index, requested_amount, balance, error_code } => {
                writer.write_u8(0)? +
                    writer.write_u8(*account_index)? +
                    writer.write_u32(*address_index)? +
                    writer.write_u64(*requested_amount)? +
                    writer.write_u64(*balance)? +
                    writer.write_u32(*error_code)?
            }
            Self::FailedWithdrawal { account_index, address_index, fee_account_index, fee_address_index, requested_amount, fee_amount, balance, balance_in_fee_token, error_code } => {
                writer.write_u8(1)? +
                    writer.write_u8(*account_index)? +
                    writer.write_u32(*address_index)? +
                    writer.write_u8(*fee_account_index)? +
                    writer.write_u32(*fee_address_index)? +
                    writer.write_u64(*requested_amount)? +
                    writer.write_u64(*fee_amount)? +
                    writer.write_u64(*balance)? +
                    writer.write_u64(*balance_in_fee_token)? +
                    writer.write_u32(*error_code)?
            }
        };

        if bytes_written > EVENT_SIZE {
            Err(io::Error::new(io::ErrorKind::Other, "Event is too large"))
        } else {
            writer.write_padding(EVENT_SIZE - bytes_written)
        }
    }
}

impl Codable for TokenState {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let account_type = AccountType::decode(reader)?;
        let version = reader.read_u32()?;
        let program_state_account = reader.read_pubkey()?;
        let token_id = reader.read_string_with_padding(MAX_TOKEN_ID_SIZE)?;

        let balances_count = reader.read_u32_as_usize()?;
        let mut balances = Vec::with_capacity(balances_count);
        for _ in 0..balances_count {
            balances.push(Balance::decode(reader)?);
        }

        Ok(Self {
            account_type,
            version,
            program_state_account,
            token_id,
            balances,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, Error> {
        let mut bytes_written = self.account_type.encode(writer)?;
        bytes_written += writer.write_u32(self.version)?;
        bytes_written += writer.write_pubkey(&self.program_state_account)?;
        bytes_written += writer.write_string_with_padding(&self.token_id, MAX_TOKEN_ID_SIZE)?;
        bytes_written += writer.write_usize_as_u32(self.balances.len())?;
        for balance in &self.balances {
            bytes_written += balance.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for ProgramState {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        let account_type = AccountType::decode(reader)?;
        let version = reader.read_u32()?;
        let withdraw_account = reader.read_pubkey()?;
        let fee_account_address = reader.read_string_with_padding(MAX_ADDRESS_SIZE)?;
        let program_change_address = reader.read_string_with_padding(MAX_ADDRESS_SIZE)?;
        let network_type = NetworkType::decode(reader)?;
        let settlement_batch_hash = reader.read_hash()?;
        let last_settlement_batch_hash = reader.read_hash()?;
        let event_count = reader.read_u16_as_usize()?;
        let mut events = Vec::with_capacity(event_count);
        for _ in 0..event_count {
            events.push(Event::decode(reader)?);
        }

        Ok(Self {
            account_type,
            version,
            withdraw_account,
            fee_account_address,
            program_change_address,
            network_type,
            settlement_batch_hash,
            last_settlement_batch_hash,
            events,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, Error> {
        let mut bytes_written = self.account_type.encode(writer)? +
            writer.write_u32(self.version)? +
            writer.write_pubkey(&self.withdraw_account)? +
            writer.write_string_with_padding(&self.fee_account_address, MAX_ADDRESS_SIZE)? +
            writer.write_string_with_padding(&self.program_change_address, MAX_ADDRESS_SIZE)? +
            self.network_type.encode(writer)? +
            writer.write_hash(&self.settlement_batch_hash)? +
            writer.write_hash(&self.last_settlement_batch_hash)? +
            writer.write_usize_as_u16(self.events.len())?;
        for event in &self.events {
            bytes_written += event.encode(writer)?;
        }
        Ok(bytes_written)
    }
}

impl Codable for WithdrawState {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            account_type: AccountType::decode(reader)?,
            version: reader.read_u32()?,
            program_state_account: reader.read_pubkey()?,
            batch_hash: reader.read_hash()?,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, Error> {
        Ok(
            self.account_type.encode(writer)? +
                writer.write_u32(self.version)? +
                writer.write_pubkey(&self.program_state_account)? +
                writer.write_hash(&self.batch_hash)?
        )
    }
}

impl Codable for RuneReceiverState {
    fn decode<R: Read + ?Sized>(reader: &mut R) -> Result<Self, Error> {
        Ok(Self {
            account_type: AccountType::decode(reader)?,
            version: reader.read_u32()?,
            program_state_account: reader.read_pubkey()?,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, Error> {
        Ok(
            self.account_type.encode(writer)? +
                writer.write_u32(self.version)? +
                writer.write_pubkey(&self.program_state_account)?
        )
    }
}

impl Codable for AccountType {
    fn decode<R: io::Read + ?Sized>(reader: &mut R) -> Result<Self, io::Error> {
        Ok(match reader.read_u8()? {
            1 => Self::Program,
            2 => Self::Token,
            3 => Self::Withdraw,
            4 => Self::RuneReceiver,
            _ => Self::Unknown,
        })
    }

    fn encode<W: Write + ?Sized>(&self, mut writer: &mut W) -> Result<usize, io::Error> {
        Ok(writer.write_u8(match self {
            Self::Program => 1,
            Self::Token => 2,
            Self::Withdraw => 3,
            Self::RuneReceiver => 4,
            Self::Unknown => 0
        })?)
    }
}


#[cfg(test)]
mod tests {
    use crate::state::*;
    use crate::instructions::*;
    use crate::serialization::Codable;

    #[test]
    fn test_instructions_serialization() {
        let instruction = ProgramInstruction::InitProgramState(InitProgramStateParams {
            fee_account: "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string(),
            program_change_address: "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_string(),
            network_type: NetworkType::Regtest,
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::InitTokenState(InitTokenStateParams {
            token_id: "BTC".to_string(),
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::InitWalletBalances(InitWalletBalancesParams {
            token_state_setups: vec![
                TokenStateSetup {
                    account_index: 0,
                    wallet_addresses: vec![
                        "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string(),
                        "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_string(),
                    ],
                },
                TokenStateSetup {
                    account_index: 1,
                    wallet_addresses: vec![
                        "33iFwdLuRpW1uK1RTRqsoi8rR4NpDzk66k".to_string(),
                        "132F25rTsvBdp9JzLLBHP5mvGY66i1xdiM".to_string(),
                    ],
                },
            ]
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::BatchDeposit(DepositBatchParams {
            token_deposits: vec![
                TokenDeposits {
                    account_index: 0,
                    deposits: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 456,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 321,
                                last4: [5, 6, 7, 8],
                            },
                            amount: 654,
                        },
                    ],
                },
                TokenDeposits {
                    account_index: 1,
                    deposits: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 222,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 333,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 444,
                        },
                    ],
                },
            ]
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::PrepareBatchWithdraw(WithdrawBatchParams {
            tx_hex: vec![1, 2, 3],
            change_amount: 123,
            input_utxo_types: vec![InputUtxoType::Bitcoin],
            token_withdrawals: vec![
                TokenWithdrawals {
                    account_index: 0,
                    fee_account_index: 0,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 456,
                            fee_address_index: AddressIndex {
                                index: 124,
                                last4: [1, 2, 3, 5],
                            },
                            fee_amount: 789,
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 321,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 654,
                            fee_address_index: AddressIndex {
                                index: 421,
                                last4: [5, 3, 2, 1],
                            },
                            fee_amount: 987,
                        },
                    ],
                },
                TokenWithdrawals {
                    account_index: 1,
                    fee_account_index: 1,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 222,
                            fee_address_index: AddressIndex {
                                index: 222,
                                last4: [1, 2, 3, 6],
                            },
                            fee_amount: 333,
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 444,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 555,
                            fee_address_index: AddressIndex {
                                index: 555,
                                last4: [1, 2, 3, 7],
                            },
                            fee_amount: 666,
                        },
                    ],
                },
            ],
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::PrepareBatchSettlement(SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 0,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 222,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 333,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 444,
                        },
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 555,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 666,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 777,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 888,
                        },
                    ],
                    fee_amount: 123,
                },
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 1111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 2222,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 3333,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 4444,
                        },
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 5555,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 6666,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 7777,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 8888,
                        },
                    ],
                    fee_amount: 1234,
                },
            ]
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::SubmitBatchSettlement(SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 0,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 222,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 333,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 444,
                        },
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 555,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 666,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 777,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 888,
                        },
                    ],
                    fee_amount: 123,
                },
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 1111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 2222,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 3333,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 4444,
                        },
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: AddressIndex {
                                index: 5555,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 6666,
                        },
                        Adjustment {
                            address_index: AddressIndex {
                                index: 7777,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 8888,
                        },
                    ],
                    fee_amount: 1234,
                },
            ]
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());

        let instruction = ProgramInstruction::RollbackBatchWithdraw(RollbackWithdrawBatchParams {
            token_withdrawals: vec![
                TokenWithdrawals {
                    account_index: 0,
                    fee_account_index: 0,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 456,
                            fee_address_index: AddressIndex {
                                index: 123,
                                last4: [1, 2, 3, 4],
                            },
                            fee_amount: 789,
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 321,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 654,
                            fee_address_index: AddressIndex {
                                index: 321,
                                last4: [4, 3, 2, 1],
                            },
                            fee_amount: 987,
                        },
                    ],
                },
                TokenWithdrawals {
                    account_index: 1,
                    fee_account_index: 1,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            amount: 222,
                            fee_address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            fee_amount: 333,
                        },
                        Withdrawal {
                            address_index: AddressIndex {
                                index: 444,
                                last4: [4, 3, 2, 1],
                            },
                            amount: 555,
                            fee_address_index: AddressIndex {
                                index: 111,
                                last4: [1, 2, 3, 4],
                            },
                            fee_amount: 666,
                        },
                    ],
                },
            ]
        });
        assert_eq!(instruction, ProgramInstruction::decode_from_slice(&instruction.encode_to_vec().unwrap()).unwrap());
    }
}

