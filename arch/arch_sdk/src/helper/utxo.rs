use bitcoin::{
    absolute::LockTime,
    address::Address,
    key::{TapTweak, TweakedKeypair},
    secp256k1::{self, Secp256k1},
    sighash::{Prevouts, SighashCache},
    transaction::Version,
    Amount, OutPoint, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, TxOut, Txid, Witness,
};
use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};

use serde_json::{from_str, Value};
use std::str::FromStr;

use crate::arch_program::pubkey::Pubkey;
use crate::constants::{
    BITCOIN_NETWORK, BITCOIN_NODE_ENDPOINT, BITCOIN_NODE_PASSWORD, BITCOIN_NODE_USERNAME,
    CALLER_FILE_PATH,
};
use crate::models::CallerInfo;

use super::get_account_address;

/* -------------------------------------------------------------------------- */
/*                             PREPARES A FEE PSBT                            */
/* -------------------------------------------------------------------------- */
/// This function sends the caller BTC, then prepares a fee PSBT and returns
/// the said PSBT in HEX encoding
pub fn prepare_fees() -> String {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let caller = CallerInfo::with_secret_key_file(CALLER_FILE_PATH)
        .expect("getting caller info should not fail");

    let txid = rpc
        .send_to_address(
            &caller.address,
            Amount::from_sat(100000),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("SATs should be sent to address");

    let sent_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("should get raw transaction");
    let mut vout: u32 = 0;

    for (index, output) in sent_tx.output.iter().enumerate() {
        if output.script_pubkey == caller.address.script_pubkey() {
            vout = index as u32;
        }
    }

    let mut tx = Transaction {
        version: Version::TWO,
        input: vec![TxIn {
            previous_output: OutPoint { txid, vout },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![],
        lock_time: LockTime::ZERO,
    };

    let sighash_type = TapSighashType::NonePlusAnyoneCanPay;
    let raw_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("raw transaction should not fail");
    let prevouts = vec![raw_tx.output[vout as usize].clone()];
    let prevouts = Prevouts::All(&prevouts);

    let mut sighasher = SighashCache::new(&mut tx);
    let sighash = sighasher
        .taproot_key_spend_signature_hash(0, &prevouts, sighash_type)
        .expect("should not fail to construct sighash");

    // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
    let secp = Secp256k1::new();
    let tweaked: TweakedKeypair = caller.key_pair.tap_tweak(&secp, None);
    let msg = secp256k1::Message::from(sighash);
    let signature = secp.sign_schnorr(&msg, &tweaked.to_inner());

    // Update the witness stack.
    let signature = bitcoin::taproot::Signature {
        signature,
        sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    tx.raw_hex()
}

/* -------------------------------------------------------------------------- */
/*               PREPARES A FEE PSBT WITH EXTRA UTXO (RBF TESTS)              */
/* -------------------------------------------------------------------------- */
/// This function sends the caller BTC, then prepares a fee PSBT and returns
/// the said PSBT in HEX encoding
pub fn prepare_fees_with_extra_utxo(rune_txid: String, rune_vout: u32) -> String {
    let rune_txid = Txid::from_str(&rune_txid).unwrap();

    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let caller = CallerInfo::with_secret_key_file(CALLER_FILE_PATH)
        .expect("getting caller info should not fail");

    let txid = rpc
        .send_to_address(
            &caller.address,
            Amount::from_sat(3000),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("SATs should be sent to address");

    let sent_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("should get raw transaction");
    let mut vout: u32 = 0;

    for (index, output) in sent_tx.output.iter().enumerate() {
        if output.script_pubkey == caller.address.script_pubkey() {
            vout = index as u32;
        }
    }

    let rune_sent_tx = rpc
        .get_raw_transaction(&rune_txid, None)
        .expect("should get raw transaction");

    let mut tx = Transaction {
        version: Version::TWO,
        input: vec![
            TxIn {
                previous_output: OutPoint {
                    txid: rune_txid,
                    vout: rune_vout,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            },
            TxIn {
                previous_output: OutPoint { txid, vout },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            },
        ],
        output: vec![TxOut {
            value: rune_sent_tx.output[rune_vout as usize].value,
            script_pubkey: ScriptBuf::from_bytes(vec![]),
        }],
        lock_time: LockTime::ZERO,
    };

    // PREPARE Prevouts

    let rune_raw_tx = rpc
        .get_raw_transaction(&rune_txid, None)
        .expect("raw transaction should not fail");

    let raw_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("raw transaction should not fail");

    let prevouts = vec![
        rune_raw_tx.output[rune_vout as usize].clone(),
        raw_tx.output[vout as usize].clone(),
    ];
    let prevouts = Prevouts::All(&prevouts);

    // Sign rune input
    let rune_sighash_type = TapSighashType::SinglePlusAnyoneCanPay;

    let mut rune_sighasher = SighashCache::new(&mut tx);

    let rune_sighash = rune_sighasher
        .taproot_key_spend_signature_hash(0, &prevouts, rune_sighash_type)
        .expect("should not fail to construct sighash");

    let secp = Secp256k1::new();
    let tweaked: TweakedKeypair = caller.key_pair.tap_tweak(&secp, None);
    let msg = secp256k1::Message::from(rune_sighash);
    let rune_signature = secp.sign_schnorr(&msg, &tweaked.to_inner());

    let rune_signature = bitcoin::taproot::Signature {
        signature: rune_signature,
        sighash_type: rune_sighash_type,
    };

    tx.input[0].witness.push(rune_signature.to_vec());

    // Sign the anchoring utxo
    let sighash_type = TapSighashType::NonePlusAnyoneCanPay;

    let mut sighasher = SighashCache::new(&mut tx);

    let sighash = sighasher
        .taproot_key_spend_signature_hash(1, &prevouts, sighash_type)
        .expect("should not fail to construct sighash");

    // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
    let secp = Secp256k1::new();
    let tweaked: TweakedKeypair = caller.key_pair.tap_tweak(&secp, None);
    let msg = secp256k1::Message::from(sighash);
    let signature = secp.sign_schnorr(&msg, &tweaked.to_inner());

    // Update the witness stack.
    let signature = bitcoin::taproot::Signature {
        signature,
        sighash_type,
    };
    tx.input[1].witness.push(signature.to_vec());

    tx.raw_hex()
}

/* -------------------------------------------------------------------------- */
/*                     SENDS A UTXO TO THE ACCOUNT ADDRESS                    */
/* -------------------------------------------------------------------------- */
/// Used to send a utxo the taptweaked account address corresponding to the
/// network's joint pubkey
pub fn send_utxo(pubkey: Pubkey) -> (String, u32) {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let _caller = CallerInfo::with_secret_key_file(CALLER_FILE_PATH)
        .expect("getting caller info should not fail");

    let address = get_account_address(pubkey);

    let account_address = Address::from_str(&address)
        .unwrap()
        .require_network(BITCOIN_NETWORK)
        .unwrap();

    let txid = rpc
        .send_to_address(
            &account_address,
            Amount::from_sat(3000),
            None,
            None,
            None,
            None,
            None,
            None,
        )
        .expect("SATs should be sent to address");

    let sent_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("should get raw transaction");
    let mut vout = 0;

    for (index, output) in sent_tx.output.iter().enumerate() {
        if output.script_pubkey == account_address.script_pubkey() {
            vout = index as u32;
        }
    }

    // let tx_info = rpc.get_raw_transaction_info(&txid, None).unwrap();

    (txid.to_string(), vout)
}

/* -------------------------------------------------------------------------- */
/*              FETCHES AN ADDRESSES RECENT UTXOS (TESTNET ONLY)              */
/* -------------------------------------------------------------------------- */
/// Given an address, this function fetches it's recent utxos.
pub fn get_address_utxos(rpc: &Client, address: String) -> Vec<Value> {
    let client = reqwest::blocking::Client::new();

    let res = client
        .get(format!(
            "https://mempool.dev.aws.archnetwork.xyz/api/address/{}/utxo",
            address
        ))
        .header("Accept", "application/json")
        .send()
        .unwrap();

    let utxos = from_str::<Value>(&res.text().unwrap()).unwrap();

    utxos
        .as_array()
        .unwrap()
        .iter()
        .filter(|utxo| {
            utxo["status"]["block_height"].as_u64().unwrap() <= rpc.get_block_count().unwrap() - 100
        })
        .cloned()
        .collect()
}
