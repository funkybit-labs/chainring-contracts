#![no_main]

use anyhow::Result;
use bitcoin::consensus;
use sdk::{entrypoint, Pubkey, UtxoInfo};

pub mod models;
pub mod processor;

use models::ExchangeInstruction;

#[cfg(target_os = "zkvm")]
entrypoint!(handler);

#[cfg(target_os = "zkvm")]
fn handler(_program_id: &Pubkey, utxos: &[UtxoInfo], instruction_data: &[u8]) -> Result<Vec<u8>> {
    let exchange_instruction: ExchangeInstruction = borsh::from_slice(instruction_data)?;
    let tx_result = match exchange_instruction {
        ExchangeInstruction::InitState(params) => processor::init_state(utxos, params),
        ExchangeInstruction::Deposit(params) => processor::deposit(utxos, params),
        ExchangeInstruction::BatchWithdraw(params) => processor::withdraw_batch(utxos, params),
        ExchangeInstruction::SubmitBatchSettlement(params) => processor::submit_settlement_batch(utxos, params),
    };

    match tx_result {
        Ok(tx) => Ok(consensus::serialize(&tx)),
        Err(e) => Err(e)
    }
}
