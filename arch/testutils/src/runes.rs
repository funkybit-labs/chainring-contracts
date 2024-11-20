use common::constants::*;
use common::models::*;
use bitcoin::{
    Address, Amount, OutPoint, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, TxOut, Witness,
    opcodes,
    absolute::LockTime,
    transaction::Version,
    secp256k1,
    secp256k1::Secp256k1,
    key::{TapTweak, TweakedKeypair},
};
use bitcoin::script::{Builder as ScriptBuilder, PushBytes};
use bitcoin::sighash::{Prevouts, SighashCache};
use bitcoin::taproot::{LeafVersion, TaprootBuilder, TapLeafHash, TaprootSpendInfo};
use bitcoincore_rpc::{Auth, Client, RpcApi};
use arch_program::pubkey::Pubkey;
use ordinals::{Edict, Etching, RuneId, Runestone};
use model::state::Balance;
use crate::bitcoin::{mine, deposit_to_address};
use crate::ordclient::{OrdClient, Output, wait_for_block};
use std::str::FromStr;
use common::helper::{get_account_address, with_secret_key_file};
use crate::constants::RUNE_RECEIVER_ACCOUNT_FILE_PATH;
use crate::setup::deposit;

pub struct ReceiverInfo<'a> {
    pub transfer_amount: u64,
    pub address: &'a Address,
}

pub fn transfer_and_deposit_runes_to_exchange(
    ord_client: &OrdClient,
    sender: &CallerInfo,
    token_account: Pubkey,
    rune_id: RuneId,
    rune_name: &str,
    deposit_amount: u64,
    expected_balance: u64,
) {
    let (_, rune_receiver_pubkey) = with_secret_key_file(RUNE_RECEIVER_ACCOUNT_FILE_PATH).unwrap();
    let rune_deposit_address = Address::from_str(&get_account_address(rune_receiver_pubkey))
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap();

    let outputs: Vec<Output> = ord_client.get_outputs_for_address(&sender.address.to_string());
    let output = outputs
        .iter()
        .find(|&x| x.runes.contains_key(rune_name) && !x.spent)
        .unwrap();

    // transfer runes
    let block = transfer_runes(
        &sender,
        rune_id,
        output,
        vec![
            ReceiverInfo {
                transfer_amount: deposit_amount,
                address: &rune_deposit_address,
            },
        ],
    );

    deposit(
        sender.address.to_string().clone(),
        &rune_id.to_string(),
        token_account,
        deposit_amount,
        vec![
            Balance {
                address: sender.address.to_string().clone(),
                balance: expected_balance,
            },
        ],
    );

    wait_for_block(&ord_client, block);
}

pub fn transfer_runes(
    sender: &CallerInfo,
    rune_id: RuneId,
    prev_output: &Output,
    receiver_infos: Vec<ReceiverInfo>,
) -> u64 {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let mut edicts: Vec<Edict> = receiver_infos
        .iter()
        .enumerate()
        .map(
            |(i, ri)|
                Edict {
                    id: rune_id,
                    amount: ri.transfer_amount as u128,
                    output: i as u32 + 1,
                },
        ).collect();

    edicts.push(
        Edict {
            id: rune_id,
            amount: 0,
            output: 0,
        },
    );
    let runestone = Runestone {
        edicts,
        etching: None,
        mint: None,
        pointer: None,
    };

    let runestone_bytes = runestone.encipher().to_bytes();
    let runestone_script = ScriptBuf::from_bytes(runestone_bytes.clone());
    let min_utxo_amount: u64 = if prev_output.value > 10000 {
        5000
    } else {
        547
    };

    // this is the 0 output - he will get the remainder
    let mut output = vec![
        TxOut {
            value: Amount::from_sat(prev_output.value - 1000 - min_utxo_amount * receiver_infos.len() as u64),
            script_pubkey: sender.address.script_pubkey(),
        },
    ];
    for ri in receiver_infos.iter() {
        output.push(
            TxOut {
                value: Amount::from_sat(min_utxo_amount),
                script_pubkey: ri.address.script_pubkey(),
            },
        )
    }
    output.push(
        TxOut {
            script_pubkey: runestone_script,
            value: Amount::from_sat(0),
        },
    );


    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![
            TxIn {
                previous_output: OutPoint::from_str(&prev_output.outpoint).unwrap(),
                script_sig: ScriptBuf::default(),
                sequence: Sequence::MAX,
                witness: Witness::default(),
            }
        ],
        output,
    };


    let binding = vec![TxOut {
        value: Amount::from_sat(prev_output.value),
        script_pubkey: sender.address.script_pubkey(),
    }];

    let prevouts = Prevouts::All(&binding);
    let sighash_type = TapSighashType::NonePlusAnyoneCanPay;
    let mut sighasher = SighashCache::new(&mut tx);
    let sighash = sighasher
        .taproot_key_spend_signature_hash(0, &prevouts, sighash_type)
        .expect("should not fail to construct sighash");

    // Sign the sighash using the secp256k1 library
    let secp = Secp256k1::new();
    let tweaked: TweakedKeypair = sender.key_pair.tap_tweak(&secp, None);
    let msg = secp256k1::Message::from(sighash);
    let signature = secp.sign_schnorr(&msg, &tweaked.to_inner());

    // Update the witness stack.
    let signature = bitcoin::taproot::Signature {
        signature,
        sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    let txid = rpc.send_raw_transaction(&tx);
    println!("sent transfer tx {:?}", txid);

    mine(1);

    rpc.get_block_count().unwrap()
}

pub fn etch_rune(
    wallet: &CallerInfo,
    etching: Etching,
    premine_address: Option<Address>
) -> RuneId {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    // Create the Runestone
    let runestone = Runestone {
        edicts: Vec::new(), // No edicts for initial etching
        etching: Some(etching),
        mint: None,
        pointer: None,
    };

    let rune_name_commitment_script = ScriptBuilder::new()
        .push_opcode(opcodes::OP_FALSE)
        .push_opcode(opcodes::all::OP_IF)
        .push_slice::<&PushBytes>(
            etching
                .rune
                .unwrap()
                .commitment()
                .as_slice()
                .try_into()
                .unwrap(),
        )
        .push_opcode(opcodes::all::OP_ENDIF)
        .push_slice(wallet.public_key.serialize())
        .push_opcode(opcodes::all::OP_CHECKSIG)
        .into_script();

    let secp = Secp256k1::new();

    // Build taproot tree
    let taproot_builder = TaprootBuilder::new()
        .add_leaf(0, rune_name_commitment_script.clone())
        .expect("error adding name commitment script leaf");

    let taproot_spend_info = taproot_builder
        .finalize(&secp, wallet.public_key)
        .expect("error finalizing taproot builder");

    // Create commit transaction output
    let commit_tx_script = ScriptBuf::new_p2tr(
        &secp,
        taproot_spend_info.internal_key(),
        taproot_spend_info.merkle_root(),
    );


    let postage = Amount::from_sat(100000);
    let commit_network_fee = Amount::from_sat(3000);
    let premine_amount = if etching.premine.is_some() { Amount::from_sat(97000) } else { Amount::ZERO };

    let (txid, vout) = deposit_to_address(postage.to_sat() + commit_network_fee.to_sat(), &wallet.address);

    // Build commit transaction
    let (commit_tx, commit_vout) = build_commit_transaction(
        &wallet,
        OutPoint { txid, vout },
        postage,
        commit_tx_script,
    );

    let mut etching_outputs = Vec::new();

    // premine the supply to the wallet
    if etching.premine.is_some() {
        etching_outputs.push(TxOut {
            script_pubkey: if let Some(address) = premine_address {
                address
            } else {
                wallet.address.clone()
            }.script_pubkey(),
            value: premine_amount,
        });
    }

    // Get the encoded runestone - already includes OP_RETURN and OP_PUSHNUM_13
    let runestone_bytes = runestone.encipher().to_bytes();
    let runestone_script = ScriptBuf::from_bytes(runestone_bytes.clone());
    // Add runestone output using runestone_script
    etching_outputs.push(TxOut {
        script_pubkey: runestone_script,
        value: Amount::from_sat(0),
    });

    // Build etching transaction with all outputs
    let etching_tx = build_etching_transaction(
        &wallet,
        &rune_name_commitment_script,
        &taproot_spend_info,
        OutPoint {
            txid: commit_tx.compute_txid(),
            vout: commit_vout,
        },
        postage,
        etching_outputs,
    );

    let _ = rpc.send_raw_transaction(&commit_tx);
    mine(6);
    let etching_txid = rpc.send_raw_transaction(&etching_tx);
    mine(1);
    let block_count = rpc.get_block_count().unwrap();
    let block = rpc.get_block_info(&rpc.get_block_hash(block_count).unwrap()).unwrap();
    RuneId {
        block: block_count,
        tx: block.tx.iter().position(|&r| r == *etching_txid.as_ref().unwrap()).unwrap() as u32,
    }
}

pub fn build_commit_transaction(
    wallet: &CallerInfo,
    prev_outpoint: OutPoint,
    amount: Amount,
    commit_script: ScriptBuf,
) -> (Transaction, u32) {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    // Create commit transaction
    let mut tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: prev_outpoint,
            script_sig: ScriptBuf::default(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        }],
        output: vec![
            TxOut {
                value: amount,
                script_pubkey: commit_script,
            },
        ],
    };

    let sighash_type = TapSighashType::NonePlusAnyoneCanPay;
    let raw_tx = rpc
        .get_raw_transaction(&prev_outpoint.txid, None)
        .expect("raw transaction should not fail");
    let prevouts = vec![raw_tx.output[prev_outpoint.vout as usize].clone()];
    let prevouts = Prevouts::All(&prevouts);

    let mut sighasher = SighashCache::new(&mut tx);
    let sighash = sighasher
        .taproot_key_spend_signature_hash(0, &prevouts, sighash_type)
        .expect("should not fail to construct sighash");

    // Sign the sighash using the secp256k1 library
    let secp = Secp256k1::new();
    let tweaked: TweakedKeypair = wallet.key_pair.tap_tweak(&secp, None);
    let msg = secp256k1::Message::from(sighash);
    let signature = secp.sign_schnorr(&msg, &tweaked.to_inner());

    // Update the witness stack.
    let signature = bitcoin::taproot::Signature {
        signature,
        sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    (tx, 0)
}

pub fn build_etching_transaction(
    wallet: &CallerInfo,
    name_commitment_script: &ScriptBuf,
    taproot_spend_info: &TaprootSpendInfo,
    commit_outpoint: OutPoint,
    prev_amount: Amount,
    additional_outputs: Vec<TxOut>,
) -> Transaction {
    let mut outputs = Vec::new();
    outputs.extend(additional_outputs);
    let mut etching_tx = Transaction {
        version: Version::TWO,
        lock_time: LockTime::ZERO,
        input: vec![TxIn {
            previous_output: commit_outpoint,
            script_sig: ScriptBuf::default(),
            sequence: Sequence::MAX,
            witness: Witness::default(),
        }],
        output: outputs,
    };

    let secp = Secp256k1::new();
    let prev_tx_out = TxOut {
        value: prev_amount,
        script_pubkey: ScriptBuf::new_p2tr(
            &secp,
            taproot_spend_info.internal_key(),
            taproot_spend_info.merkle_root(),
        ),
    };

    let mut sighash_cache = SighashCache::new(&mut etching_tx);
    let leaf_hash = TapLeafHash::from_script(name_commitment_script, LeafVersion::TapScript);
    let sighash = sighash_cache
        .taproot_script_spend_signature_hash(
            0,
            &Prevouts::All(&[prev_tx_out]),
            leaf_hash,
            TapSighashType::Default,
        )
        .expect("Failed to construct sighash");

    let signature = secp.sign_schnorr(
        &secp256k1::Message::from_digest_slice(sighash.as_ref()).unwrap(),
        &wallet.key_pair,
    );

    let witness = sighash_cache
        .witness_mut(0)
        .expect("getting mutable witness reference should work");

    witness.push(signature.as_ref());
    witness.push(name_commitment_script);
    witness.push(
        &taproot_spend_info
            .control_block(&(name_commitment_script.clone(), LeafVersion::TapScript))
            .expect("Failed to create control block")
            .serialize(),
    );

    etching_tx
}
