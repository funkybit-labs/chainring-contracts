use anyhow::{anyhow, Result};
use serde::Serialize;
use serde_json::{from_str, json, Value};

use crate::processed_transaction::ProcessedTransaction;

use crate::arch_program::{message::Message, pubkey::Pubkey, system_instruction};

use crate::constants::{
    GET_BEST_BLOCK_HASH, GET_BLOCK, GET_PROCESSED_TRANSACTION, NODE1_ADDRESS,
    TRANSACTION_NOT_FOUND_CODE,
};
use crate::runtime_transaction::{RuntimeTransaction, RUNTIME_TX_SIZE_LIMIT};
use crate::signature::Signature;

/* -------------------------------------------------------------------------- */
/*              RETRIEVES A PROCESSED TRANSACTION FROM VALIDATOR              */
/* -------------------------------------------------------------------------- */
/// This endpoint is used to retrieve a processed transaction from db, it is
/// mainly used to inquire about a transaction's execution and if it's
/// processed successfully.
/// Keeps trying for a maximum of 60 seconds
pub fn get_processed_transaction(url: &str, tx_id: String) -> Result<ProcessedTransaction> {
    let mut processed_tx =
        process_get_transaction_result(post_data(url, GET_PROCESSED_TRANSACTION, tx_id.clone()));
    if let Err(e) = processed_tx {
        return Err(anyhow!("{}", e));
    }

    let mut wait_time = 1;
    while let Ok(Value::Null) = processed_tx {
        std::thread::sleep(std::time::Duration::from_secs(wait_time));
        processed_tx = process_get_transaction_result(post_data(
            url,
            GET_PROCESSED_TRANSACTION,
            tx_id.clone(),
        ));
        wait_time += 1;
        if wait_time >= 60 {
            println!("get_processed_transaction has run for more than 60 seconds");
            return Err(anyhow!("Failed to retrieve processed transaction"));
        }
    }

    if let Ok(ref tx) = processed_tx {
        let mut p = tx.clone();

        let get_status = |p: Value| -> String {
            if p["status"].as_str().is_some() {
                p["status"].as_str().unwrap().to_string()
            } else if let Some(val) = p["status"].as_object() {
                if val.contains_key("Failed") {
                    "Failed".to_string()
                } else {
                    unreachable!("WTFFF");
                }
            } else {
                unreachable!("WTFFF2");
            }
        };

        while get_status(p.clone()) != *"Processed" && get_status(p.clone()) != *"Failed" {
            std::thread::sleep(std::time::Duration::from_secs(wait_time));
            p = process_get_transaction_result(post_data(
                url,
                GET_PROCESSED_TRANSACTION,
                tx_id.clone(),
            ))
            .unwrap();
            wait_time += 10;
            if wait_time >= 60 {
                println!("get_processed_transaction has run for more than 60 seconds");
                return Err(anyhow!("Failed to retrieve processed transaction"));
            }
        }
        processed_tx = Ok(p);
    }

    Ok(serde_json::from_value(processed_tx?).unwrap())
}

/* -------------------------------------------------------------------------- */
/*                  MAX LENGTH FOR AN ACCOUNT DATA EXTENSION                  */
/* -------------------------------------------------------------------------- */
/// Returns the remaining space in an account's data storage
pub fn extend_bytes_max_len() -> usize {
    let message = Message {
        signers: vec![Pubkey::system_program()],
        instructions: vec![system_instruction::write_bytes(
            0,
            0,
            vec![0_u8; 8],
            Pubkey::system_program(),
        )],
    };

    RUNTIME_TX_SIZE_LIMIT
        - RuntimeTransaction {
            version: 0,
            signatures: vec![Signature([0_u8; 64].to_vec())],
            message,
        }
        .serialize()
        .len()
}

/* -------------------------------------------------------------------------- */
/*                     REQUEST DISTRIBUTED KEY GENERATION                     */
/* -------------------------------------------------------------------------- */
/// Starts a Distributed Key Generation round by calling the RPC method, if a
/// key has already been generated, fails.
pub fn start_dkg() {
    if let Err(err) = process_result(post(NODE1_ADDRESS, "start_dkg")) {
        println!("Error starting DKG: {:?}", err);
    };
}

/* -------------------------------------------------------------------------- */
/*                           Returns the best block                           */
/* -------------------------------------------------------------------------- */
/// Returns the latest block hash from the Arch blockchain
pub fn get_best_block() -> String {
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

/* -------------- Response processing and decoding functions : -------------- */

pub fn process_result(response: String) -> Result<Value> {
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

pub fn process_get_transaction_result(response: String) -> Result<Value> {
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

pub fn post(url: &str, method: &str) -> String {
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

pub fn post_data<T: Serialize + std::fmt::Debug>(url: &str, method: &str, params: T) -> String {
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
        .send();

    res.expect("post method should not fail")
        .text()
        .expect("result should be text decodable")
}

/* -------------- Response processing and decoding functions : -------------- */

/*
pub async fn get_program_accounts(
    context: Arc<ValidatorContext>,
    program_id: Pubkey,
    filters: Option<Vec<AccountFilter>>,
) -> Result<Vec<ProgramAccount>, ErrorObject<'static>> {
    match context
        .rocks_db()
        .await
        .get_program_accounts(&program_id, filters)
    {
        Ok(accounts) => Ok(accounts),
        Err(err) => {
            error!("Error fetching program accounts: {:?}", err);
            Err(ErrorObject::borrowed(
                ErrorCode::InternalError.code(),
                "Error fetching program accounts",
                None,
            ))
        }
    }
}
*/
