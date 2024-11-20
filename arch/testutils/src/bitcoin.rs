use common::constants::*;
use std::str::FromStr;
use bitcoin::{Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, Txid, TxIn, Witness, absolute::LockTime, transaction::Version, TapSighashType, secp256k1};
use bitcoin::key::{Secp256k1, TweakedKeypair, TapTweak};
use bitcoin::sighash::{Prevouts, SighashCache};
use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};
use common::models::CallerInfo;

pub fn deposit_to_address(
    amount: u64,
    address: &Address,
) -> (Txid, u32) {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let txid = rpc
        .send_to_address(
            address,
            Amount::from_sat(amount),
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
        if output.script_pubkey == address.script_pubkey() {
            vout = index as u32;
        }
    }
    return (txid, vout);
}

pub fn prepare_withdrawal(
    amount: u64,
    estimated_fee: u64,
    txid: &str,
    vout: u32,
) -> (String, u64) {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let txid = Txid::from_str(txid).unwrap();
    let raw_tx = rpc
        .get_raw_transaction(&txid, None)
        .expect("raw transaction should not fail");

    let prev_output = raw_tx.output[vout as usize].clone();

    if amount + estimated_fee > prev_output.value.to_sat() {
        panic!("not enough in utxo to cover amount and fee")
    }

    let change_amount = prev_output.value.to_sat() - amount - estimated_fee;

    let tx = Transaction {
        version: Version::TWO,
        input: vec![TxIn {
            previous_output: OutPoint {
                txid,
                vout,
            },
            script_sig: ScriptBuf::new(),
            sequence: Sequence::MAX,
            witness: Witness::new(),
        }],
        output: vec![],
        lock_time: LockTime::ZERO,
    };

    (tx.raw_hex(), change_amount)
}

pub fn mine(num_blocks: u64) {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let generate_to_address = Address::from_str("bcrt1q3nyukkpkg6yj0y5tj6nj80dh67m30p963mzxy7")
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap();
    rpc
        .generate_to_address(num_blocks, &generate_to_address)
        .expect("failed to mine block");
}

pub fn get_block() -> u64 {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );

    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    rpc.get_block_count().expect("should not fail to get block count")
}

pub fn prepare_fees(caller: &CallerInfo) -> Transaction {
    let userpass = Auth::UserPass(
        BITCOIN_NODE_USERNAME.to_string(),
        BITCOIN_NODE_PASSWORD.to_string(),
    );
    let rpc =
        Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

    let network = match BITCOIN_NETWORK {
        bitcoin::Network::Bitcoin => bitcoincore_rpc::bitcoin::Network::Bitcoin,
        bitcoin::Network::Testnet => bitcoincore_rpc::bitcoin::Network::Testnet,
        bitcoin::Network::Signet => bitcoincore_rpc::bitcoin::Network::Signet,
        bitcoin::Network::Regtest => bitcoincore_rpc::bitcoin::Network::Regtest,
        _ => panic!("Unsupported bitcoin network type"),
    };

    let address = bitcoincore_rpc::bitcoin::Address::from_str(&caller.address.to_string())
        .expect("failed to parse address")
        .require_network(network)
        .expect("invalid network");

    let txid = rpc
        .send_to_address(
            &address,
            bitcoincore_rpc::bitcoin::Amount::from_sat(3000),
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
        if output.script_pubkey
            == bitcoincore_rpc::bitcoin::ScriptBuf::from(caller.address.script_pubkey())
        {
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

    tx
}