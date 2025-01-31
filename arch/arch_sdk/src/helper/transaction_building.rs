use anyhow::{anyhow, Result};
use bitcoin::{
    key::{Keypair, UntweakedKeypair},
    XOnlyPublicKey,
};
use indicatif::{ProgressBar, ProgressStyle};
use serde_json::Value;

use crate::arch_program::pubkey::Pubkey;
use crate::constants::{BITCOIN_NETWORK, NODE1_ADDRESS};
use crate::runtime_transaction::RuntimeTransaction;
use crate::signature::Signature;
use crate::{arch_program::instruction::Instruction, processed_transaction::ProcessedTransaction};
use crate::{
    arch_program::message::Message, constants::GET_PROCESSED_TRANSACTION,
    processed_transaction::Status,
};

use super::{post_data, process_get_transaction_result, process_result, sign_message_bip322};
/* -------------------------------------------------------------------------- */
/*                      INSTRUCTION CREATION AND SENDING                      */
/* -------------------------------------------------------------------------- */
/// Creates an instruction, signs it as a message and sends the signed message
/// as a transaction to the configured node
pub fn sign_and_send_instruction(
    instruction: Instruction,
    signers: Vec<Keypair>,
) -> Result<(String, String)> {
    // Step 1: Get public keys from signers
    let pubkeys = signers
        .iter()
        .map(|signer| Pubkey::from_slice(&XOnlyPublicKey::from_keypair(signer).0.serialize()))
        .collect::<Vec<Pubkey>>();

    // Step 2: Create a message with the instruction and signers
    let message = Message {
        signers: pubkeys.clone(), // Clone for logging purposes
        instructions: vec![instruction.clone()],
    };

    // Step 3: Hash the message and decode
    let digest_slice = message.hash();

    // Step 4: Sign the message with each signer's key
    let signatures = signers
        .iter()
        .map(|signer| {
            let signature = sign_message_bip322(signer, &digest_slice, BITCOIN_NETWORK).to_vec();
            Signature(signature)
        })
        .collect::<Vec<Signature>>();

    // Step 5: Create transaction parameters
    let params = RuntimeTransaction {
        version: 0,
        signatures: signatures.clone(), // Clone for logging purposes
        message: message.clone(),       // Clone for logging purposes
    };

    // Step 6: Send transaction to node for processeing
    let result = process_result(post_data(NODE1_ADDRESS, "send_transaction", params))
        .expect("send_transaction should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string();

    // Step 7: Hash the instruction
    let hashed_instruction = instruction.hash();

    Ok((result, hashed_instruction))
}

/* -------------------------------------------------------------------------- */
/*                 MULTIPLE INSTRUCTION CREATION AND SENDING                  */
/* -------------------------------------------------------------------------- */
/// Creates a transaction from provided instructions, signs it as a message
/// and sends the signed message as a transaction to the configured node
pub fn sign_and_send_transaction(
    instructions: Vec<Instruction>,
    signers: Vec<UntweakedKeypair>,
) -> Result<String> {
    let pubkeys = signers
        .iter()
        .map(|signer| Pubkey::from_slice(&XOnlyPublicKey::from_keypair(signer).0.serialize()))
        .collect::<Vec<Pubkey>>();

    let message = Message {
        signers: pubkeys,
        instructions,
    };
    let digest_slice = message.hash();
    let signatures = signers
        .iter()
        .map(|signer| {
            Signature(sign_message_bip322(signer, &digest_slice, BITCOIN_NETWORK).to_vec())
        })
        .collect::<Vec<Signature>>();

    let params = RuntimeTransaction {
        version: 0,
        signatures,
        message,
    };
    let result = process_result(post_data(NODE1_ADDRESS, "send_transaction", params))
        .expect("send_transaction should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string();

    Ok(result)
}

/* -------------------------------------------------------------------------- */
/*                   BUILDS A TRANSACTION FROM INSTRUCTIONS                   */
/* -------------------------------------------------------------------------- */
/// Builds a runtime transaction given a set of instructions.
pub fn build_transaction(
    signer_key_pairs: Vec<Keypair>,
    instructions: Vec<Instruction>,
) -> RuntimeTransaction {
    let pubkeys = signer_key_pairs
        .iter()
        .map(|signer| Pubkey::from_slice(&XOnlyPublicKey::from_keypair(signer).0.serialize()))
        .collect::<Vec<Pubkey>>();

    let message = Message {
        signers: pubkeys,
        instructions,
    };

    let digest_slice = message.hash();

    let signatures = signer_key_pairs
        .iter()
        .map(|signer| {
            let signature = sign_message_bip322(signer, &digest_slice, BITCOIN_NETWORK).to_vec();
            Signature(signature)
        })
        .collect::<Vec<Signature>>();

    RuntimeTransaction {
        version: 0,
        signatures,
        message,
    }
}

/* -------------------------------------------------------------------------- */
/*                  BUILDS AND SENDS A BATCH OF TRANSACTIONS                  */
/* -------------------------------------------------------------------------- */
/// Given a set of runtime transactions, batches and transfers them to the validator
pub fn build_and_send_block(transactions: Vec<RuntimeTransaction>) -> Vec<String> {
    let result: bitcoincore_rpc::jsonrpc::serde_json::Value =
        process_result(post_data(NODE1_ADDRESS, "send_transactions", transactions))
            .expect("send_transaction should not fail");

    let transaction_ids: Vec<String> =
        bitcoincore_rpc::jsonrpc::serde_json::from_value(result).expect("Couldn't decode response");

    transaction_ids
}

/* -------------------------------------------------------------------------- */
/*                       RETRIEVES A SET OF TRANSACTIONS                      */
/* -------------------------------------------------------------------------- */
/// Retrieves a vec of processed transactions, awaiting each processing for 60
/// secs max
pub fn fetch_processed_transactions(
    transaction_ids: Vec<String>,
) -> Result<Vec<ProcessedTransaction>> {
    let pb = ProgressBar::new(transaction_ids.len() as u64);

    pb.set_style(ProgressStyle::default_bar()
            .progress_chars("x>-")
            .template("{spinner:.green}[{elapsed_precise:.blue}] {pos:.blue} {msg:.blue} [{bar:100.green/blue}] {pos}/{len} ({eta})").unwrap());

    pb.set_message("Fetched Processed Transactions :");

    let mut processed_transactions: Vec<ProcessedTransaction> = vec![];

    for transaction_id in transaction_ids.iter() {
        let mut wait_time = 1;

        let mut processed_tx = process_get_transaction_result(post_data(
            NODE1_ADDRESS,
            GET_PROCESSED_TRANSACTION,
            transaction_id.clone(),
        ))
        .unwrap();

        while processed_tx == Value::Null {
            std::thread::sleep(std::time::Duration::from_secs(wait_time));
            processed_tx = process_get_transaction_result(post_data(
                NODE1_ADDRESS,
                GET_PROCESSED_TRANSACTION,
                transaction_id.clone(),
            ))
            .unwrap();
            wait_time += 1;
            if wait_time >= 60 {
                println!("get_processed_transaction has run for more than 60 seconds");
                return Err(anyhow!("Failed to retrieve processed transaction"));
            }
        }

        while Status::from_value(&processed_tx["status"]) == Some(Status::Queued) {
            //println!("Processed transaction is not yet finalized. Retrying...");
            std::thread::sleep(std::time::Duration::from_secs(wait_time));
            processed_tx = process_get_transaction_result(post_data(
                NODE1_ADDRESS,
                GET_PROCESSED_TRANSACTION,
                transaction_id.clone(),
            ))
            .unwrap();
            wait_time += 1;
            if wait_time >= 60 {
                println!("get_processed_transaction has run for more than 60 seconds");
                return Err(anyhow!("Failed to retrieve processed transaction"));
            }
        }
        processed_transactions.push(serde_json::from_value(processed_tx).unwrap());
        pb.inc(1);
        pb.set_message("Fetched Processed Transactions :");
    }
    pb.finish();

    Ok(processed_transactions)
}
