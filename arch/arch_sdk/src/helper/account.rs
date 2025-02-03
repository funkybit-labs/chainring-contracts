use anyhow::{anyhow, Result};

use bitcoin::key::Keypair;

use serde::Deserialize;
use serde::Serialize;

use crate::arch_program::pubkey::Pubkey;
use crate::arch_program::system_instruction;
use crate::constants::NODE1_ADDRESS;
use crate::constants::READ_ACCOUNT_INFO;

use super::{get_processed_transaction, post_data, process_result, sign_and_send_instruction};

/* -------------------------------------------------------------------------- */
/*                  ASSIGN AN ACCOUNT OWNERSHIP TO A PROGRAM                  */
/* -------------------------------------------------------------------------- */
/// Used to assign an account's ownership to another pubkey, requires current
/// owner's key pair.
pub fn assign_ownership_to_program(
    program_pubkey: &Pubkey,
    account_to_transfer_pubkey: Pubkey,
    current_owner_keypair: Keypair,
) {
    let mut instruction_data = vec![3];
    instruction_data.extend(program_pubkey.serialize());

    let (txid, _) = sign_and_send_instruction(
        system_instruction::assign(account_to_transfer_pubkey, *program_pubkey),
        vec![current_owner_keypair],
    )
    .expect("signing and sending a transaction should not fail");

    let _processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
        .expect("get processed transaction should not fail");
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AccountInfoResult {
    pub owner: Pubkey,
    pub data: Vec<u8>,
    pub utxo: String,
    pub is_executable: bool,
}

/* -------------------------------------------------------------------------- */
/*                      Reading data stored in an Account                     */
/* -------------------------------------------------------------------------- */
/// This endpoint is used to inquire about the date stored within an account, the
/// validator checks its DB and returns the data within the account, Utxo details,
/// executability and latest tag.
pub fn read_account_info(url: &str, pubkey: Pubkey) -> Result<AccountInfoResult> {
    let result = process_result(post_data(url, READ_ACCOUNT_INFO, pubkey))?;
    serde_json::from_value(result).map_err(|_| anyhow!("Unable to decode read_account_info result"))
}
