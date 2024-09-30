use sha2::{Digest, Sha256};
use solana_nostd_secp256k1_recover::secp256k1_recover;
use solana_secp256k1_schnorr::challenges::bip340::BIP340Challenge;
use crate::{address::AddressType, sha256_ripemd160, double_sha256};
use solana_secp256k1_schnorr::Secp256k1SchnorrSignature;
use solana_secp256k1::{CompressedPoint, Secp256k1Point, UncompressedPoint};

pub struct Transaction {
    pub version: i32,
    pub inputs: Vec<TransactionInput>,
    pub outputs: Vec<TransactionOutput>,
}

pub struct TransactionInput {
    pub outpoint: TransactionOutPoint,
    pub script_sig: Vec<u8>,
    pub sequence: u32,
}

pub struct TransactionOutPoint {
    pub txid: Vec<u8>,
    pub vout: u32,
}

pub struct TransactionOutput {
    pub value: u64,
    pub script_pubkey: Vec<u8>,
}

pub fn get_virtual_tx(msg: &[u8], script: &[u8]) -> Transaction {
    // Build transaction to spend
    let mut tx_to_spend = Transaction {
        version: 0,
        inputs: Vec::new(),
        outputs: Vec::new(),
    };

    // Add input to tx_to_spend
    let dummy_tx_hash = vec!(0u8).repeat(32);
    let input = TransactionInput {
        outpoint: TransactionOutPoint {
            txid: dummy_tx_hash,
            vout: 0xffffffff,
        },
        script_sig: Vec::new(),
        sequence: 0x00000000,
    };

    // Add output to tx_to_spend
    let output = TransactionOutput {
        value: 0,
        script_pubkey: script.to_vec(),
    };
    tx_to_spend.outputs.push(output);


    // Build the message hash
    let message_bytes = msg.to_vec();
    let bip0322_tag = b"BIP0322-signed-message";
    let msg_hash = sha256(&[get_tap_tag(bip0322_tag), message_bytes].concat());

    // Sign the input
    let mut script_sig = vec![0x00];
    script_sig.extend_from_slice(op_push_data(&msg_hash).as_slice());
    let mut input_with_sig = input;
    input_with_sig.script_sig = script_sig;
    tx_to_spend.inputs.push(input_with_sig);

    // Build transaction to sign
    let mut tx_to_sign = Transaction {
        version: 0,
        inputs: Vec::new(),
        outputs: Vec::new(),
    };

    // Add input to tx_to_sign
    let input_to_sign = TransactionInput {
        outpoint: TransactionOutPoint {
            txid: tx_hash(&tx_to_spend),
            vout: 0,
        },
        script_sig: script.to_vec(),
        sequence: 0x00000000,
    };
    tx_to_sign.inputs.push(input_to_sign);

    // Add OP_RETURN output to tx_to_sign
    let op_return_output = TransactionOutput {
        value: 0,
        script_pubkey: vec![0x6a], // OP_RETURN
    };
    tx_to_sign.outputs.push(op_return_output);

    tx_to_sign
}

fn sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

fn get_tap_tag(tag: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(tag);
    hasher.finalize().to_vec().repeat(2)
}


pub fn tx_hash(tx: &Transaction) -> Vec<u8> {
    let serialized = serialize_tx(tx);
    double_sha256(&serialized)
}

pub fn serialize_tx(tx: &Transaction) -> Vec<u8> {
    let mut result = Vec::new();

    // Serialize version (4 bytes, little endian)
    result.extend_from_slice(&tx.version.to_le_bytes());

    // Serialize number of inputs (VarInt)
    result.extend(serialize_varint(tx.inputs.len() as u64));

    // Serialize each input
    for input in &tx.inputs {
        result.extend_from_slice(&input.outpoint.txid.as_slice());
        result.extend_from_slice(&input.outpoint.vout.to_le_bytes());
        result.extend(serialize_varint(input.script_sig.len() as u64));
        result.extend_from_slice(&input.script_sig);
        result.extend_from_slice(&input.sequence.to_le_bytes());
    }

    // Serialize number of outputs (VarInt)
    result.extend(serialize_varint(tx.outputs.len() as u64));

    // Serialize each output
    for output in &tx.outputs {
        result.extend_from_slice(&output.value.to_le_bytes());
        result.extend(serialize_varint(output.script_pubkey.len() as u64));
        result.extend_from_slice(&output.script_pubkey);
    }

    // Serialize locktime (4 bytes, little endian)
    result.extend_from_slice(&[0, 0, 0, 0]); // Assuming locktime is always 0 in this implementation

    result
}

pub fn serialize_varint(value: u64) -> Vec<u8> {
    if value < 0xfd {
        vec![value as u8]
    } else if value <= 0xffff {
        let mut result = vec![0xfd];
        result.extend_from_slice(&(value as u16).to_le_bytes());
        result
    } else if value <= 0xffffffff {
        let mut result = vec![0xfe];
        result.extend_from_slice(&(value as u32).to_le_bytes());
        result
    } else {
        let mut result = vec![0xff];
        result.extend_from_slice(&value.to_le_bytes());
        result
    }
}

fn op_push_data(data: &[u8]) -> Vec<u8> {
    let len = data.len();
    if len < 76 {
        [&[len as u8], data].concat()
    } else if len < 256 {
        [&[76, len as u8], data].concat()
    } else if len < 65536 {
        [&[77u8], (len as u16).to_le_bytes().as_slice(), data].concat()
    } else {
        [&[78u8], (len as u32).to_le_bytes().as_slice(), data].concat()
    }
}

fn generate_single_sig_script(pubkey: &[u8], address_type: AddressType) -> Vec<u8> {
    match address_type {
        AddressType::P2TR => {
            let mut script = Vec::new();
            script.extend_from_slice(&op_push_data(pubkey));
            script.push(0xac); // OP_CHECKSIG
            script
        },
        _ => {
            let pubkey_hash = sha256_ripemd160(pubkey);
            let mut script = Vec::new();
            script.push(0x76); // OP_DUP
            script.push(0xa9); // OP_HASH160
            script.extend_from_slice(&op_push_data(&pubkey_hash));
            script.push(0x88); // OP_EQUALVERIFY
            script.push(0xac); // OP_CHECKSIG
            script
        },
    }
}

#[derive(Debug)]
struct ECDSASignature {
    rs: [u8; 64],
    v: u8,
}

fn der_to_rsv(der_sig: &[u8]) -> Result<ECDSASignature, &'static str> {
    if der_sig.len() < 9 {  // Minimum length: header(2) + r(3) + s(3) + sighash(1)
        return Err("DER signature too short");
    }

    if der_sig[0] != 0x30 {
        return Err("Invalid DER signature: missing header byte");
    }

    let total_len = der_sig[1] as usize;
    if total_len + 3 != der_sig.len() {  // +3 for header and SigHash
        return Err("Invalid DER signature: length mismatch");
    }

    let mut index = 2;

    // Parse r
    if der_sig[index] != 0x02 {
        return Err("Invalid DER signature: missing integer marker for r");
    }
    index += 1;

    let r_len = der_sig[index] as usize;
    index += 1;
    let r_start = index;
    index += r_len;

    // Parse s
    if der_sig[index] != 0x02 {
        return Err("Invalid DER signature: missing integer marker for s");
    }
    index += 1;

    let s_len = der_sig[index] as usize;
    index += 1;
    let s_start = index;

    // Extract v (SigHash type) from the last byte
    let v = *der_sig.last().ok_or("Missing SigHash type")?;

    // Convert r and s to 32-byte arrays
    let mut rs = [0u8; 64];

    let r_slice = &der_sig[r_start..r_start + r_len];
    let s_slice = &der_sig[s_start..s_start + s_len];

    rs[32 - r_slice.len()..32].copy_from_slice(r_slice);
    rs[64 - s_slice.len()..].copy_from_slice(s_slice);

    Ok(ECDSASignature { rs, v })
}

pub fn verify_p2wpkh_signature(tx_to_sign: &Transaction, signature: &[u8]) -> bool {
    let sig_len = signature[1] as usize;
    let sig = &signature[2..sig_len + 2];
    let pubkey = &signature[sig_len + 3..];
    let script_code = generate_single_sig_script(pubkey, AddressType::P2WPKH);

    let zero4 = vec![0u8; 4];
    let zero8 = vec![0u8; 8];

    // Witness message prefix
    let witness_msg_prefix = [
        &zero4[..],
        &double_sha256(&[
            &tx_to_sign.inputs[0].outpoint.txid[..],
            &zero4[..],
        ].concat())[..],
        &double_sha256(&zero4)[..],
    ].concat();

    // Witness message suffix
    let output_script = &tx_to_sign.outputs[0].script_pubkey;
    let witness_msg_suffix = [
        &double_sha256(&[
            &zero8[..],
            &serialize_varint(output_script.len() as u64)[..],
            output_script,
        ].concat())[..],
        &zero4[..],
    ].concat();

    let msg_to_hash = &[
        &witness_msg_prefix[..],
        // outpoint
        &tx_to_sign.inputs[0].outpoint.txid[..],
        &zero4[..],
        // script code
        &serialize_varint(script_code.len() as u64)[..],
        &script_code[..],
        // value
        &zero8[..],
        // sequence
        &zero4[..],
        &witness_msg_suffix[..],
        // sig hash
        &[1, 0, 0, 0],
    ].concat();

    // Full message hash
    let msg_hash = double_sha256(msg_to_hash);
    let mut msg_hash_bytes = [0u8; 32];
    msg_hash_bytes.copy_from_slice(&msg_hash.as_slice());

    if let Ok(sig) = der_to_rsv(sig) {
        if let Ok(recovered) = secp256k1_recover(&msg_hash_bytes, sig.v % 2 == 1, &sig.rs) {
            let compressed = UncompressedPoint(recovered).compress().0;
            return pubkey.eq(compressed.as_slice());
        }
    }

    false
}

pub fn verify_p2tr_signature(tx_to_sign: &Transaction, signature: &[u8], script: &[u8]) -> bool {
    if signature.len() != 66 || script.len() != 34 {
        return false;
    }
    let sig = &signature[2..66];
    let pubkey = &script[1..];

    let tx_to_send = &tx_to_sign.inputs[0];
    let output_script = &tx_to_sign.outputs[0].script_pubkey;

    let zero4 = [0u8; 4];
    let zero8 = [0u8; 8];

    let sig_msg = [
        // hashType
        &[0u8][..],
        // transaction
        // version
        &zero4[..],
        // locktime
        &zero4[..],
        // prevoutHash
        &sha256(&[
            &tx_to_sign.inputs[0].outpoint.txid[..],
            &zero4[..],
        ].concat())[..],
        // amountHash
        &sha256(&zero8)[..],
        // scriptPubKeyHash
        &sha256(&[
            &serialize_varint(tx_to_send.script_sig.len() as u64)[..],
            &tx_to_send.script_sig[..],
        ].concat())[..],
        // sequenceHash
        &sha256(&zero4)[..],
        // outputHash
        &sha256(&[
            &zero8[..],
            &serialize_varint(output_script.len() as u64)[..],
            output_script,
        ].concat())[..],
        // inputs
        // spend type
        &[0u8][..],
        // input idx
        &zero4[..],
    ].concat();

    let msg_hash = sha256(&[
        &get_tap_tag("TapSighash".as_bytes())[..],
        &[0u8][..],
        &sig_msg[..],
    ].concat());

    let mut sig_bytes = [0u8; 64];
    sig_bytes.copy_from_slice(sig);
    let schnorr_signature = Secp256k1SchnorrSignature(sig_bytes);
    let mut pubkey_bytes = [0u8; 33];
    pubkey_bytes.copy_from_slice(pubkey);
    let compressed_point = CompressedPoint(pubkey_bytes.into());
    schnorr_signature.verify::<BIP340Challenge, CompressedPoint>(&msg_hash.as_slice(), &compressed_point).is_ok()
}

