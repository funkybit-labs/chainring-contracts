use anyhow::{anyhow, Result};
use arch_program::pubkey::Pubkey;

use bitcoin::{
    address::Address,
    key::UntweakedKeypair,
    secp256k1::{Secp256k1, SecretKey},
    XOnlyPublicKey,
};
use rand_core::OsRng;

use std::{fs, str::FromStr};

use crate::constants::{BITCOIN_NETWORK, GET_ACCOUNT_ADDRESS, NODE1_ADDRESS};

use super::{post_data, process_result};

/* -------------------------------------------------------------------------- */
/*                           GENERATES A NEW KEYPAIR                          */
/* -------------------------------------------------------------------------- */
/// Generates an untweaked keypair, and provides it's pubkey and BTC address
/// corresponding to the currently used BTC Network
pub fn generate_new_keypair() -> (UntweakedKeypair, Pubkey, Address) {
    let secp = Secp256k1::new();

    let (secret_key, _public_key) = secp.generate_keypair(&mut OsRng);

    let key_pair = UntweakedKeypair::from_secret_key(&secp, &secret_key);

    let (x_only_public_key, _parity) = XOnlyPublicKey::from_keypair(&key_pair);

    let address = Address::p2tr(&secp, x_only_public_key, None, BITCOIN_NETWORK);

    let pubkey = Pubkey::from_slice(&XOnlyPublicKey::from_keypair(&key_pair).0.serialize());

    (key_pair, pubkey, address)
}

/* -------------------------------------------------------------------------- */
/*                           Fetching the group key                           */
/* -------------------------------------------------------------------------- */
/// This endpoint is used to fetch the network's account address, this is the joint
/// address corresponding to the shared public key of all the validators
/// on the network and the tweak of the account pubkey
/// Utxos are sent to the network using this particular address
pub fn get_account_address(pubkey: Pubkey) -> String {
    process_result(post_data(
        NODE1_ADDRESS,
        GET_ACCOUNT_ADDRESS,
        pubkey.serialize(),
    ))
    .expect("get_account_address should not fail")
    .as_str()
    .expect("cannot convert result to string")
    .to_string()
}

pub fn with_secret_key_file(file_path: &str) -> Result<(UntweakedKeypair, Pubkey)> {
    let secp = Secp256k1::new();

    let file_content = fs::read_to_string(file_path);

    let secret_key = match file_content {
        Ok(key) => SecretKey::from_str(&key).unwrap_or_else(|_| {
            let secret_bytes: Vec<u8> = serde_json::from_str(&key).unwrap_or_else(|_| {
                panic!("File content is neither a valid secret key string nor a serialized vector of bytes");
            });

            SecretKey::from_slice(&secret_bytes[0..32]).expect("Failed to parse secret key from bytes")
        }),
        Err(_) => {
            let (key, _) = secp.generate_keypair(&mut OsRng);
            fs::write(file_path, key.display_secret().to_string())
                .map_err(|_| anyhow!("Unable to write file"))?;
            key
        }
    };
    let keypair = UntweakedKeypair::from_secret_key(&secp, &secret_key);
    let pubkey = Pubkey::from_slice(&XOnlyPublicKey::from_keypair(&keypair).0.serialize());

    Ok((keypair, pubkey))
}
