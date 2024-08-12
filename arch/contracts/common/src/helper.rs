//! This module contains helper methods for interacting with the HelloWorld program

use anyhow::{anyhow, Result};
use bitcoin::{absolute::LockTime, address::Address, key::{TapTweak, TweakedKeypair}, opcodes::all::OP_RETURN, secp256k1::{self, Secp256k1}, sighash::{Prevouts, SighashCache}, transaction::Version, Amount, OutPoint, ScriptBuf, Sequence, TapSighashType, Transaction, TxIn, TxOut, Witness, Txid};
use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};
use serde::{Serialize, Deserialize};
use borsh::{BorshSerialize, BorshDeserialize};
use serde_json::{from_str, json, Value};
use std::collections::HashMap;
use std::fs;
use std::str::FromStr;
use risc0_zkvm::Receipt;

use sdk::{Pubkey, Instruction, UtxoMeta, RuntimeTransaction, Message, Signature};

use crate::constants::{BITCOIN_NODE_ENDPOINT, BITCOIN_NODE_PASSWORD, BITCOIN_NODE_USERNAME, CALLER_FILE_PATH, FAUCET_ADDR, GET_BEST_BLOCK_HASH, GET_BLOCK, GET_CONTRACT_ADDRESS, GET_PROCESSED_TRANSACTION, GET_PROGRAM, NODE1_ADDRESS, READ_UTXO, SUBMITTER_FILE_PATH, TRANSACTION_NOT_FOUND_CODE, ASSIGN_AUTHORITY};
use crate::models::{AssignAuthorityParams, AuthorityMessage, CallerInfo, DeployProgramParams, ReadUtxoParams, Utxo};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct ReadUtxoResult {
    pub utxo_id: String,
    pub data: Vec<u8>,
    pub authority: Pubkey,
}

#[derive(Clone, Debug, Deserialize, Serialize, BorshDeserialize, BorshSerialize)]
pub enum Status {
    Processing,
    Success,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct ProcessedTransaction {
    pub runtime_transaction: RuntimeTransaction,
    pub receipts: HashMap<String, Receipt>,
    pub status: Status,
    pub bitcoin_txids: HashMap<String, String>,
}

fn process_result(response: String) -> Result<Value> {
    let result = from_str::<Value>(&response).expect("result should be Value parseable");

    let result = match result {
        Value::Object(object) => object,
        _ => panic!("unexpected output"),
    };

    if let Some(err) = result.get("error") {
        return Err(anyhow!("{:?}", err));
    }

    Ok(result["result"].clone())
}

fn process_get_transaction_result(response: String) -> Result<Value> {
    let result = from_str::<Value>(&response).expect("result should be string parseable");

    let result = match result {
        Value::Object(object) => object,
        _ => panic!("unexpected output"),
    };

    if let Some(err) = result.get("error") {
        if let Value::Number(code) = result["error"]["code"].clone() {
            if code.as_i64() == Some(TRANSACTION_NOT_FOUND_CODE) {
                return Ok(Value::Null);
            }
        }
        return Err(anyhow!("{:?}", err));
    }

    Ok(result["result"].clone())
}

fn post(url: &str, method: &str) -> String {
    let client = reqwest::blocking::Client::new();
    let res = client
        .post(url)
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": "curlycurl",
            "method": method,
        }))
        .send()
        .expect("post method should not fail");

    res.text().expect("result should be text decodable")
}

fn post_data<T: Serialize + std::fmt::Debug>(url: &str, method: &str, params: T) -> String {
    let client = reqwest::blocking::Client::new();
    let res = client
        .post(url)
        .header("content-type", "application/json")
        .json(&json!({
            "jsonrpc": "2.0",
            "id": "curlycurl",
            "method": method,
            "params": params,
        }))
        .send()
        .expect("post method should not fail");

    res.text().expect("result should be text decodable")
}

/// Creates an instruction, signs it as a message
/// and sends the signed message as a transaction
pub fn sign_and_send_instruction(
    program_id: Pubkey,
    utxos: Vec<UtxoMeta>,
    instruction_data: Vec<u8>,
) -> Result<(String, String)> {
    mine();

    let caller = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH)?;

    let instruction = Instruction {
        program_id,
        utxos,
        data: instruction_data,
    };

    let message = Message {
        signers: vec![Pubkey::from_slice(&caller.public_key.serialize())],
        instructions: vec![instruction.clone()],
    };
    let digest_slice = hex::decode(message.hash().expect("message should be hashable"))
        .expect("hashed message should be decodable");
    let sig_message = secp256k1::Message::from_digest_slice(&digest_slice)
        .expect("signed message should be gotten from digest slice");

    let secp = Secp256k1::new();
    let sig = secp.sign_schnorr(&sig_message, &caller.key_pair);

    let params = RuntimeTransaction {
        version: 0,
        signatures: vec![Signature(sig.serialize().to_vec())],
        message,
    };
    let result = process_result(post_data(NODE1_ADDRESS, "send_transaction", params))
        .expect("send_transaction should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string();
    let hashed_instruction = instruction
        .hash()
        .expect("instruction hashing should not fail");

    Ok((result, hashed_instruction))
}

/// Creates an instruction, signs it as a message
/// and sends the signed message as a transaction
pub fn assign_authority(
    program_id: Pubkey,
    utxo: UtxoMeta,
    value: u64
) -> Result<String> {
    let caller = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH)?;

    let message = AuthorityMessage {
        utxo: Utxo {
            txid: utxo.txid,
            vout: utxo.vout,
            value: value
        },
        data: vec![],
        authority: program_id,
    };
    let digest_slice = hex::decode(message.hash().unwrap())
        .expect("hashed message should be decodable");
    let sig_message = secp256k1::Message::from_digest_slice(&digest_slice)
        .expect("signed message should be gotten from digest slice");

    let secp = Secp256k1::new();
    let sig = secp.sign_schnorr(&sig_message, &caller.key_pair);

    let params = AssignAuthorityParams {
        signature: Signature(sig.serialize().to_vec()),
        message: message,
    };

    let result = process_result(post_data(NODE1_ADDRESS, ASSIGN_AUTHORITY, params))
        .expect("assign_autority should not fail")
        .as_str()
        .expect("assign_authority should not fail")
        .to_string();

    Ok(result)
}

/// Deploys the HelloWorld program using the compiled ELF
pub fn deploy_program() -> String {
    let elf = fs::read("target/program.elf").expect("elf path should be available");
    let params = DeployProgramParams { elf };
    process_result(post_data(NODE1_ADDRESS, "deploy_program", params))
        .expect("deploy_program should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string()
}

/// Starts Key Exchange by calling the RPC method
pub fn start_key_exchange() {
    match process_result(post(NODE1_ADDRESS, "start_key_exchange")) {
        Err(err) => println!("Error starting Key Exchange: {:?}", err),
        Ok(val) => assert!(val.as_bool().unwrap())
    };
}

/// Starts a Distributed Key Generation round by calling the RPC method
pub fn start_dkg() {
    if let Err(err) = process_result(post(NODE1_ADDRESS, "start_dkg")) {
        println!("Error starting DKG: {:?}", err);
    };
}

/// Read Utxo given the utxo ID
pub fn read_utxo(utxo_id: String) -> Result<ReadUtxoResult> {
    let params = ReadUtxoParams { utxo_id };
    let result = process_result(post_data(NODE1_ADDRESS, READ_UTXO, params))
        .expect("read_utxo should not fail");
    serde_json::from_value(result).map_err(|_| anyhow!("Unable to decode read_utxo result"))
}

/// Returns a program given the program ID
pub fn get_program(program_id: String) -> Vec<u8> {
    use std::convert::TryFrom;
    process_result(post_data(NODE1_ADDRESS, GET_PROGRAM, program_id))
        .expect("get_program should not fail")
        .as_array()
        .expect("cannot convert result to array")
        .into_iter()
        .map(|v| u8::try_from(v.as_u64().unwrap()).ok().unwrap())
        .collect()
}

/// Returns the best block
#[allow(dead_code)]
fn get_best_block() -> String {
    let best_block_hash = process_result(post(NODE1_ADDRESS, GET_BEST_BLOCK_HASH))
        .expect("best_block_hash should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string();
    process_result(post_data(NODE1_ADDRESS, GET_BLOCK, best_block_hash))
        .expect("get_block should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string()
}

/// Returns a processed transaction given the txid
/// Keeps trying for a maximum of 60 seconds if the processed transaction is not available
pub fn get_processed_transaction(url: &str, tx_id: String) -> Result<ProcessedTransaction> {
    let mut processed_tx = process_get_transaction_result(post_data(url, GET_PROCESSED_TRANSACTION, tx_id.clone()));
    if let Err(e) = processed_tx {
        return Err(anyhow!("{}", e));
    }

    let interval = 2;
    let mut wait_time = 2;
    while let Ok(Value::Null) = processed_tx {
        println!("Processed transaction is not yet in the database. Retrying...");
        std::thread::sleep(std::time::Duration::from_secs(interval));
        processed_tx = process_get_transaction_result(post_data(url, GET_PROCESSED_TRANSACTION, tx_id.clone()));
        wait_time += interval;
        if wait_time >= 10 {
            println!("get_processed_transaction has run for more than 60 seconds");
            return Err(anyhow!("Failed to retrieve processed transaction"));
        }
    }

    wait_time = 2;
    if let Ok(ref tx) = processed_tx {
        let mut p = tx.clone();
        while p["status"].as_str().unwrap() != "Success" {
            println!("Processed transaction is not yet finalized. Retrying...");
            std::thread::sleep(std::time::Duration::from_secs(interval));
            p = process_get_transaction_result(post_data(url, GET_PROCESSED_TRANSACTION, tx_id.clone())).unwrap();
            wait_time += interval;
            if wait_time >= 10 {
                println!("get_processed_transaction has run for more than 60 seconds");
                return Err(anyhow!("Failed to retrieve processed transaction"));
            }
        }
        processed_tx = Ok(p);
    }

    Ok(serde_json::from_value::<ProcessedTransaction>(processed_tx?).unwrap())
}

pub fn mine() {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let _ = rpc.generate_to_address(1, &Address::from_str(FAUCET_ADDR).unwrap().require_network(bitcoin::Network::Regtest).unwrap());
}

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
        if output.script_pubkey == caller.address.script_pubkey() {
            vout = index as u32;
        }
    }

    mine();

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
        sig: signature,
        hash_ty: sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    tx.raw_hex()
}

pub fn prepare_deposit(
    signer: &str,
    amount: u64,
    estimated_fee: u64,
    _program_id: Pubkey,
) -> String {

    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let caller = CallerInfo::with_secret_key_file(signer)
        .expect("getting caller info should not fail");

    let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH)
        .expect("getting submitter info should not fail");

    let txid = rpc
        .send_to_address(
            &caller.address,
            Amount::from_sat(amount + estimated_fee),
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
        if output.script_pubkey == caller.address.script_pubkey() {
            vout = index as u32;
        }
    }

    let network_address = get_arch_bitcoin_address();

    let mut tx = Transaction {
        version: Version::TWO,
        input: vec![TxIn {
            previous_output: OutPoint { txid, vout },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(0),
                script_pubkey: ScriptBuf::builder()
                    .push_opcode(OP_RETURN)
                    .push_x_only_key(&submitter.public_key)
                    .into_script()
            },
            TxOut {
                value: Amount::from_sat(amount),
                script_pubkey: network_address.script_pubkey(),
            },
        ],
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
        sig: signature,
        hash_ty: sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    tx.raw_hex()
}

pub fn prepare_withdrawal(
    wallet: &str,
    amount: u64,
    estimated_fee: u64,
    utxo_meta: UtxoMeta
) -> String {

    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let wallet = CallerInfo::with_secret_key_file(wallet)
        .expect("getting caller info should not fail");

    let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH)
        .expect("getting submitter info should not fail");

    let network_address = get_arch_bitcoin_address();

    let txid = Txid::from_str(&utxo_meta.txid).unwrap();
    let raw_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("raw transaction should not fail");

    let prev_output = raw_tx.output[utxo_meta.vout as usize].clone();

    if amount + estimated_fee > prev_output.value.to_sat() {
        panic!("not enough in utxo to cover amount and fee")
    }

    let mut outputs = vec![
        TxOut {
            value: Amount::from_sat(amount),
            script_pubkey: wallet.address.script_pubkey(),
        },
    ];
    if amount + estimated_fee < prev_output.value.to_sat() {
        outputs.push(TxOut {
            value: Amount::from_sat(0),
            script_pubkey: ScriptBuf::builder()
                .push_opcode(OP_RETURN)
                .push_x_only_key(&submitter.public_key)
                .into_script(),
        });
        outputs.push(
            TxOut {
                value: Amount::from_sat(prev_output.value.to_sat() - amount - estimated_fee),
                script_pubkey: network_address.script_pubkey(),
            }
        );
    }

    let mut tx = Transaction {
        version: Version::TWO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid: txid,
                vout: utxo_meta.vout
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: outputs,
        lock_time: LockTime::ZERO,
    };


    let sighash_type = TapSighashType::NonePlusAnyoneCanPay;
    let prevouts = vec![prev_output];
    let prevouts = Prevouts::All(&prevouts);

    let mut sighasher = SighashCache::new(&mut tx);
    let sighash = sighasher
        .taproot_key_spend_signature_hash(0, &prevouts, sighash_type)
        .expect("should not fail to construct sighash");

    // Sign the sighash using the secp256k1 library (exported by rust-bitcoin).
    let secp = Secp256k1::new();
    let tweaked: TweakedKeypair = submitter.key_pair.tap_tweak(&secp, None);
    let msg = secp256k1::Message::from(sighash);
    let signature = secp.sign_schnorr(&msg, &tweaked.to_inner());

    // Update the witness stack.
    let signature = bitcoin::taproot::Signature {
        sig: signature,
        hash_ty: sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    tx.raw_hex()
}


pub fn send_utxo(
    signer: &str
) -> String {

    mine();
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let caller = CallerInfo::with_secret_key_file(signer)
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
    let mut vout = 0;

    for (index, output) in sent_tx.output.iter().enumerate() {
        if output.script_pubkey == caller.address.script_pubkey() {
            vout = index as u32;
        }
    }

    let network_address = get_network_address("");

    let mut tx = Transaction {
        version: Version::TWO,
        input: vec![TxIn {
            previous_output: OutPoint { txid, vout },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![
            TxOut {
                value: Amount::from_sat(0),
                script_pubkey: ScriptBuf::builder()
                    .push_opcode(OP_RETURN)
                    .push_x_only_key(&caller.public_key)
                    .into_script(),
            },
            TxOut {
                value: Amount::from_sat(1500),
                script_pubkey: Address::from_str(&network_address)
                    .unwrap()
                    .require_network(bitcoin::Network::Regtest)
                    .unwrap()
                    .script_pubkey(),
            },
        ],
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
        sig: signature,
        hash_ty: sighash_type,
    };
    tx.input[0].witness.push(signature.to_vec());

    // BOOM! Transaction signed and ready to broadcast.
    rpc.send_raw_transaction(tx.raw_hex())
        .expect("sending raw transaction should not fail")
        .to_string()
}

pub fn get_raw_transaction(state_txid: &str) -> Transaction {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");
    let raw_tx = rpc
        .get_raw_transaction(&Txid::from_str(state_txid).unwrap(), None)
        .expect("raw transaction should not fail");

    raw_tx
}

fn get_network_address(data: &str) -> String {
    let mut params = HashMap::new();
    params.insert("data", data.as_bytes());
    process_result(post_data(NODE1_ADDRESS, GET_CONTRACT_ADDRESS, params))
        .expect("get_contract_address should not fail")
        .as_str()
        .expect("cannot convert result to string")
        .to_string()
}

pub fn get_arch_bitcoin_address() -> Address {
    Address::from_str(&get_network_address(""))
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap()
}

#[allow(dead_code)]
fn get_address_utxos(rpc: &Client, address: String) -> Vec<Value> {
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
        .map(|utxo| utxo.clone())
        .collect()
}

