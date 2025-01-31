use anyhow::Result;

use indicatif::{ProgressBar, ProgressStyle};
use tracing::debug;

use std::fs;

use crate::helper::{
    get_processed_transaction, print_title, send_utxo, sign_and_send_instruction,
    with_secret_key_file,
};

use crate::arch_program::message::Message;
use crate::arch_program::pubkey::Pubkey;
use crate::arch_program::system_instruction;
use crate::constants::{BITCOIN_NETWORK, NODE1_ADDRESS};
use crate::error::SDKError;
use crate::runtime_transaction::RuntimeTransaction;
use crate::signature::Signature;

use bitcoin::key::UntweakedKeypair;
use bitcoin::XOnlyPublicKey;

use super::{extend_bytes_max_len, post_data, process_result, sign_message_bip322};
use crate::helper::read_account_info;
/* -------------------------------------------------------------------------- */
/*                             PROGRAM DEPLOYMENT                             */
/* -------------------------------------------------------------------------- */
/// Tries to deploy the program

pub fn try_deploy_program(
    elf_path: &str,
    program_file_path: &str,
    program_name: &str,
) -> anyhow::Result<arch_program::pubkey::Pubkey> {
    print_title(&format!("PROGRAM DEPLOYMENT {}", program_name), 5);

    let (program_keypair, program_pubkey) =
        with_secret_key_file(program_file_path).expect("getting caller info should not fail");

    let elf = fs::read(elf_path).expect("elf path should be available");

    if let Ok(account_info_result) = read_account_info(NODE1_ADDRESS, program_pubkey) {
        if account_info_result.data == elf {
            println!("\x1b[33m Same program already deployed ! Skipping deployment. \x1b[0m");
            print_title(
                &format!(
                    "PROGRAM DEPLOYMENT : OK Program account : {:?} !",
                    program_pubkey.0
                ),
                5,
            );
            return Ok(program_pubkey);
        }
        println!("\x1b[33m ELF mismatch with account content ! Redeploying \x1b[0m");
    };

    let (deploy_utxo_btc_txid, deploy_utxo_vout) = send_utxo(program_pubkey);

    println!(
        "\x1b[32m Step 1/4 Successful :\x1b[0m BTC Transaction for program account UTXO successfully sent : https://mempool.dev.aws.archnetwork.xyz/tx/{} -- vout : {}",
        deploy_utxo_btc_txid, deploy_utxo_vout
    );

    let (pa_arch_txid, _pa_arch_txid_hash) = sign_and_send_instruction(
        system_instruction::create_account(
            hex::decode(deploy_utxo_btc_txid)
                .unwrap()
                .try_into()
                .unwrap(),
            deploy_utxo_vout,
            program_pubkey,
        ),
        vec![program_keypair],
    )
    .expect("signing and sending a transaction should not fail");

    let _processed_tx = get_processed_transaction(NODE1_ADDRESS, pa_arch_txid.clone())
        .expect("get processed transaction should not fail");

    println!("\x1b[32m Step 2/4 Successful :\x1b[0m Program account creation transaction successfully processed ! Tx Id : {}.\x1b[0m",pa_arch_txid.clone());

    deploy_program_txs(program_keypair, elf_path)?;

    let elf = fs::read(elf_path).expect("elf path should be available");

    let program_info_after_deployment = read_account_info(NODE1_ADDRESS, program_pubkey).unwrap();

    assert!(program_info_after_deployment.data == elf);

    debug!(
        "Current Program Account {:x}: \n   Owner : {}, \n   Data length : {} Bytes,\n   Anchoring UTXO : {}, \n   Executable? : {}",
        program_pubkey, program_info_after_deployment.owner,
        program_info_after_deployment.data.len(),
        program_info_after_deployment.utxo,
        program_info_after_deployment.is_executable
    );

    println!("\x1b[32m Step 3/4 Successful :\x1b[0m Sent ELF file as transactions, and verified program account's content against local ELF file!");

    let (executability_txid, _) = sign_and_send_instruction(
        system_instruction::deploy(program_pubkey),
        vec![program_keypair],
    )
    .expect("signing and sending a transaction should not fail");

    let _processed_tx = get_processed_transaction(NODE1_ADDRESS, executability_txid.clone())
        .expect("get processed transaction should not fail");

    let program_info_after_making_executable =
        read_account_info(NODE1_ADDRESS, program_pubkey).unwrap();

    debug!(
        "Current Program Account {:x}: \n   Owner : {:x}, \n   Data length : {} Bytes,\n   Anchoring UTXO : {}, \n   Executable? : {}",
        program_pubkey,
        program_info_after_making_executable.owner,
        program_info_after_making_executable.data.len(),
        program_info_after_making_executable.utxo,
        program_info_after_making_executable.is_executable
    );

    assert!(program_info_after_making_executable.is_executable);

    println!("\x1b[32m Step 4/4 Successful :\x1b[0m Made program account executable!");

    print_title(
        &format!(
            "PROGRAM DEPLOYMENT : OK Program account : {:?} !",
            program_pubkey.0
        ),
        5,
    );

    println!("\x1b[33m\x1b[1m Program account Info :\x1b[0m");
    println!(
        "\x1b[33mAccount Pubkey : \x1b[0m {} // {}",
        hex::encode(program_pubkey.0),
        program_pubkey,
    );
    println!(
        "\x1b[33mOwner : \x1b[0m{} // {:?}",
        hex::encode(program_info_after_making_executable.owner.0),
        program_info_after_making_executable.owner.0,
    );
    println!(
        "\x1b[33m\x1b[1mIs executable : \x1b[0m{}",
        program_info_after_making_executable.is_executable
    );
    println!(
        "\x1b[33m\x1b[1mUtxo details : \x1b[0m{}",
        program_info_after_making_executable.utxo
    );
    println!(
        "\x1b[33m\x1b[1mELF Size : \x1b[0m{} Bytes",
        program_info_after_making_executable.data.len()
    );

    Ok(program_pubkey)
}

/// Deploys the HelloWorld program using the compiled ELF
pub fn deploy_program_txs(
    program_keypair: UntweakedKeypair,
    elf_path: &str,
) -> Result<(), SDKError> {
    let program_pubkey =
        Pubkey::from_slice(&XOnlyPublicKey::from_keypair(&program_keypair).0.serialize());

    let account_info = read_account_info(NODE1_ADDRESS, program_pubkey).map_err(|e| {
        SDKError::FromStrError(format!("Read account info failed : {}", e.to_string()).to_string())
    })?;

    if account_info.is_executable {
        let (txid, _) = sign_and_send_instruction(
            system_instruction::retract(program_pubkey),
            vec![program_keypair],
        )
        .map_err(|_| SDKError::SignAndSendFailed)?;

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .map_err(|_| SDKError::GetProcessedTransactionFailed)?;

        println!("processed_tx {:?}", processed_tx);
    }

    let elf = fs::read(elf_path).map_err(|_| SDKError::ElfPathNotFound)?;

    if account_info.data.len() > elf.len() {
        let (txid, _) = sign_and_send_instruction(
            system_instruction::truncate(program_pubkey, elf.len() as u32),
            vec![program_keypair],
        )
        .map_err(|_| SDKError::SignAndSendFailed)?;

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .map_err(|_| SDKError::GetProcessedTransactionFailed)?;

        println!("processed_tx {:?}", processed_tx);
    }

    let txs = elf
        .chunks(extend_bytes_max_len())
        .enumerate()
        .map(|(i, chunk)| {
            let offset: u32 = (i * extend_bytes_max_len()) as u32;
            let len: u32 = chunk.len() as u32;

            let message = Message {
                signers: vec![program_pubkey],
                instructions: vec![system_instruction::write_bytes(
                    offset,
                    len,
                    chunk.to_vec(),
                    program_pubkey,
                )],
            };

            let digest_slice = message.hash();

            RuntimeTransaction {
                version: 0,
                signatures: vec![Signature(
                    sign_message_bip322(&program_keypair, &digest_slice, BITCOIN_NETWORK).to_vec(),
                )],
                message,
            }
        })
        .collect::<Vec<RuntimeTransaction>>();

    let post_result = post_data(NODE1_ADDRESS, "send_transactions", txs);
    let processed_data =
        process_result(post_result).map_err(|_| SDKError::SendTransactionFailed)?;
    let array_data = processed_data
        .as_array()
        .ok_or(SDKError::InvalidResponseType)?;
    let txids = array_data
        .iter()
        .map(|r| {
            r.as_str()
                .ok_or(SDKError::InvalidResponseType)
                .map(String::from)
        })
        .collect::<Result<Vec<String>, SDKError>>()?;

    let pb = ProgressBar::new(txids.len() as u64);

    pb.set_style(ProgressStyle::default_bar()
        .progress_chars("#>-")
        .template("{spinner:.green}[{elapsed_precise:.blue}] {msg:.blue} [{bar:100.green/blue}] {pos}/{len} ({eta})").unwrap());

    pb.set_message("Successfully Processed Deployment Transactions :");

    for txid in txids {
        let _processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .map_err(|_| SDKError::GetProcessedTransactionFailed)?;
        pb.inc(1);
        pb.set_message("Successfully Processed Deployment Transactions :");
    }

    pb.finish();
    Ok(())
}
