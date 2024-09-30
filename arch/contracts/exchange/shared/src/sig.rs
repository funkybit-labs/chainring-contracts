extern crate alloc;

use alloc::vec::Vec;
use alloc::string::String;
//use k256::{ecdsa::{RecoveryId, Signature, VerifyingKey}, EncodedPoint, PublicKey};
use base58::{FromBase58, ToBase58};
use bech32::{segwit};
use crate::address::{pubkey_hash_to_segwit_address, AddressType, BitcoinAddress, Network};
use crate::bip322::{get_virtual_tx, serialize_varint, verify_p2tr_signature, verify_p2wpkh_signature};
use crate::{parse_recovery_id, sha256_ripemd160};
use solana_nostd_secp256k1_recover::secp256k1_recover;
use solana_secp256k1::{Secp256k1Point, UncompressedPoint};

pub struct BitcoinSignatureVerification;

const OP_1: u8 = 0x51;
const OP_CHECKMULTISIG: u8 = 0xae;
const MAINNET_P2PKH: u8 = 0x00;
const MAINNET_P2SH: u8 = 0x05;
const TESTNET_P2PKH: u8 = 0x6F;
const TESTNET_P2SH: u8 = 0xC4;

impl BitcoinSignatureVerification {
    pub fn verify_message(address: &BitcoinAddress, signature_bytes: &[u8], message_bytes: &[u8]) -> bool {
        if signature_bytes.len() == 65 {
            // Legacy ECDSA signature
            Self::verify_ecdsa_signature(&address, &signature_bytes, message_bytes)
        } else {
            // BIP322 signature (Segwit P2WPKH or Taproot P2TR)
            Self::verify_bip322_signature(&address.address, &signature_bytes, message_bytes)
        }
    }

    fn verify_ecdsa_signature(address: &BitcoinAddress, signature_bytes: &[u8], message: &[u8]) -> bool {
        let message_hash = crate::double_sha256(&Self::bitcoin_message_magic(message));

        let first_byte = signature_bytes[0];
        let recovery_parsed = parse_recovery_id(first_byte).unwrap();
        let mut signature: [u8; 64] = [0u8; 64];
        signature.copy_from_slice(&signature_bytes[1..65]);
        let mut message_hash_bytes: [u8; 32] = [0u8; 32];
        message_hash_bytes.copy_from_slice(message_hash.as_slice());

        if let Ok(public_key_bytes) = secp256k1_recover(&message_hash_bytes, recovery_parsed.1, &signature) {
            let compressed = UncompressedPoint(public_key_bytes).compress();
            let address_from_key = Self::public_key_to_address(&compressed.0, &address);
            address_from_key == address.address
        } else {
            false
        }
    }

    fn verify_bip322_signature(address: &str, signature: &[u8], message: &[u8]) -> bool {
        let script = Self::address_to_script(address).unwrap();
        let tx_to_sign = get_virtual_tx(message, &script);

        match script[0] {
            0x00 => verify_p2wpkh_signature(&tx_to_sign, signature),
            0x51 => verify_p2tr_signature(&tx_to_sign, signature, &script),
            _ => false,
        }
    }

    fn bitcoin_message_magic(message: &[u8]) -> Vec<u8> {
        let prefix = b"\x18Bitcoin Signed Message:\n";
        let mut result = Vec::new();
        result.extend_from_slice(prefix);
        result.extend_from_slice(&serialize_varint(message.len() as u64));
        result.extend_from_slice(message);
        result
    }

    fn create_p2sh_address(script_hash: &[u8], network: &Network) -> String {
        let version_byte = match network {
            Network::Mainnet => MAINNET_P2SH,
            Network::Testnet => TESTNET_P2SH,
        };
        let mut address = vec![version_byte];
        address.extend_from_slice(script_hash);
        let checksum = &crate::double_sha256(&address)[..4];
        address.extend_from_slice(checksum);
        address.to_base58()
    }

    fn public_key_to_address(public_key: &[u8], expected_address: &BitcoinAddress) -> String {
        let pubkey_hash = sha256_ripemd160(public_key);

        match expected_address.address_type {
            AddressType::P2WPKH => {
                pubkey_hash_to_segwit_address(pubkey_hash.as_slice(), &expected_address).unwrap()
            }
            AddressType::P2SH => {
                let mut script = vec![OP_1, 33]; // OP_1 and pubkey length
                script.extend_from_slice(public_key);
                script.extend_from_slice(&[81, OP_CHECKMULTISIG]); // number of pubkeys as small num and OP_CHECKMULTISIG
                let script_hash = sha256_ripemd160(script.as_slice());
                let p2sh_address = Self::create_p2sh_address(&script_hash, &expected_address.network);
                if p2sh_address == expected_address.address {
                    p2sh_address
                } else {
                    // p2sh-p2wpkh
                    let mut p2wpkh_script = vec![0, 20]; // OP_0 and pubkey hash length
                    p2wpkh_script.extend_from_slice(pubkey_hash.as_slice());
                    let p2sh_p2wpkh_script_hash = sha256_ripemd160(p2wpkh_script.as_slice());
                    Self::create_p2sh_address(&p2sh_p2wpkh_script_hash, &expected_address.network)
                }
            }
            _ => {
                let version_byte = match expected_address.network {
                    Network::Mainnet => MAINNET_P2PKH,
                    Network::Testnet => TESTNET_P2PKH,
                };

                let mut address = vec![version_byte];
                address.extend_from_slice(&pubkey_hash);

                // Perform double SHA256
                let checksum = &crate::double_sha256(&address)[..4];

                // Append checksum
                address.extend_from_slice(checksum);

                // Encode with Base58
                address.to_base58()
            }
        }
    }

    pub fn address_to_script(address: &str) -> Result<Vec<u8>, &'static str> {
        if address.starts_with('1') || address.starts_with('m') || address.starts_with('n') {
            // P2PKH
            Self::p2pkh_address_to_script(address)
        } else if address.starts_with('3') || address.starts_with('2') {
            // P2SH
            Self::p2sh_address_to_script(address)
        } else if address.starts_with("bc1q") || address.starts_with("tb1q") || address.starts_with("bcrt1q") {
            // P2WPKH
            Self::p2wpkh_address_to_script(address)
        } else if address.starts_with("bc1p") || address.starts_with("tb1p") || address.starts_with("bcrt1p") {
            // P2TR
            Self::p2tr_address_to_script(address)
        } else {
            Err("Unsupported address format")
        }
    }

    fn p2pkh_address_to_script(address: &str) -> Result<Vec<u8>, &'static str> {
        let decoded = address.from_base58().map_err(|_| "Invalid Base58 encoding")?;
        if decoded.len() != 25 {
            return Err("Invalid P2PKH address length");
        }
        if decoded[0] != 0x00 {
            return Err("Invalid P2PKH address version");
        }
        let pub_key_hash = &decoded[1..21];

        let mut script = Vec::with_capacity(25);
        script.push(0x76); // OP_DUP
        script.push(0xa9); // OP_HASH160
        script.push(0x14); // Push 20 bytes
        script.extend_from_slice(pub_key_hash);
        script.push(0x88); // OP_EQUALVERIFY
        script.push(0xac); // OP_CHECKSIG

        Ok(script)
    }

    fn p2sh_address_to_script(address: &str) -> Result<Vec<u8>, &'static str> {
        let decoded = address.from_base58().map_err(|_| "Invalid Base58 encoding")?;
        if decoded.len() != 25 {
            return Err("Invalid P2SH address length");
        }
        if decoded[0] != 0x05 {
            return Err("Invalid P2SH address version");
        }
        let script_hash = &decoded[1..21];

        let mut script = Vec::with_capacity(23);
        script.push(0xa9); // OP_HASH160
        script.push(0x14); // Push 20 bytes
        script.extend_from_slice(script_hash);
        script.push(0x87); // OP_EQUAL

        Ok(script)
    }

    fn p2wpkh_address_to_script(address: &str) -> Result<Vec<u8>, &'static str> {
        let (_hrp, witness_version, witness_program) = segwit::decode(address).map_err(|_| "Invalid Bech32 encoding")?;
        if witness_version.to_u8() != 0u8 {
            return Err("Invalid witness version")
        }

        let mut script = Vec::with_capacity(22);
        script.push(0x00); // OP_0
        script.push(0x14); // Push 20 bytes
        script.extend_from_slice(witness_program.as_slice());

        Ok(script)
    }

    fn p2tr_address_to_script(address: &str) -> Result<Vec<u8>, &'static str> {
        let (_hrp, witness_version, witness_program) = segwit::decode(address).map_err(|_| "Invalid Bech32m encoding")?;
        if witness_version.to_u8() != 1u8 {
            return Err("Invalid witness version")
        }

        let mut script = Vec::with_capacity(34);
        script.push(0x51); // OP_1
        script.push(0x20); // Push 32 bytes
        script.extend_from_slice(witness_program.as_slice());

        Ok(script)
    }
}

#[cfg(test)]
mod tests {
    use base64::Engine;
    use base64::engine::general_purpose;
    use super::*;
    use env_logger;

    fn verification_test(address: &str, message_to_sign: &str, signature: &str) {
        let _ = env_logger::builder().is_test(true).try_init();

        let bitcoin_address = BitcoinAddress::new(address).unwrap();

        assert!(BitcoinSignatureVerification::verify_message(
            &bitcoin_address,
            general_purpose::STANDARD.decode(signature).unwrap().as_slice(),
            message_to_sign.as_bytes())
        );
    }

    #[test]
    fn test_p2sh_signature_verification() {
        verification_test(
            "2MuBP8G3ZaKYCvrhicoM7v5hmhLcebPFsdi",
            "[funkybit] Please sign this message to verify your ownership of this wallet address. This action will not cost any gas fees.\nAddress: 2MuBP8G3ZaKYCvrhicoM7v5hmhLcebPFsdi, Timestamp: 2024-08-21T14:14:13.095Z",
            "JDQ7fNjw2JOQUeTzlMCLhesFfS+AHMkbwQAX7cbUNjQLUQSO2YuX62KwLHrhjfQMO0EjBJ5BAqCb/OfW9CBCTsg=",
        );
    }

    #[test]
    fn test_p2pkh_signature_verification() {
        verification_test(
            "muCDftUr7MxYTtkuQ3NcD1GFL3p18jEQU8",
            "[funkybit] Please sign this message to verify your ownership of this wallet address. This action will not cost any gas fees.\nAddress: 1EgGNqPsJLXHgnHHgUQEP63vU4DJKEkpiq, Timestamp: 2024-08-21T14:20:17.358Z",
            "IH8bsp/iwsft8DvqD6I6aq0rt2dr2spQAl4Y7udt19N1Wl+v5djary8UxXs+rAErvvY/niYxURJJAd2rukydNCg=",
        );
    }

    #[test]
    fn test_segwit_65bytes_signature_verification() {
        verification_test(
            "bcrt1qmasw66mddrrkwdumd24lece7hslyy303xwk8nv",
            "[funkybit] Please sign this message to verify your ownership of this wallet address. This action will not cost any gas fees.\nAddress: bcrt1qmasw66mddrrkwdumd24lece7hslyy303xwk8nv, Timestamp: 2024-08-19T20:22:52.523Z",
            "IBfCHqORRWn4cMSzC4+Vt8DDmFLf64hkW0DDfZxDa0hOX9esApXWZvOECDVmG2adny8Z3NebIhmC6zhN3HTTTdc=",
        );
    }

    #[test]
    fn test_segwit_signature_verification() {
        verification_test(
            "bc1qw5p0htg7n4cfezyck7pkygk89nrx5yhttwcgg0",
            "[funkybit] Please sign this message to verify your ownership of this wallet address. This action will not cost any gas fees.\nAddress: bc1qw5p0htg7n4cfezyck7pkygk89nrx5yhttwcgg0, Timestamp: 2024-08-19T20:22:52.523Z",
            "AkcwRAIgIcIApbLuQeAikBSVgnSAArxQiCmntysG0jd6f9aAO3gCIEohertG+SIpwJS4EGDkBaSyUktN2E958II7JMFUmNf0ASEDwd8wX2HIQflk3mxf3fQpTMeiqoyMcJ6EEcmFYzOvvZ0=",
        );
    }

    #[test]
    fn test_taproot_signature_verification() {
        verification_test(
            "bc1pyu06hqa927kff4v2kgdct7vew69w6wn7yphtz4emqj0qyrjgfrusvy6fz9",
            "[funkybit] Please sign this message to verify your ownership of this wallet address. This action will not cost any gas fees.\nAddress: bc1pyu06hqa927kff4v2kgdct7vew69w6wn7yphtz4emqj0qyrjgfrusvy6fz9, Timestamp: 2024-08-19T20:22:52.523Z",
            "AUDbsQk0Q1p2T0sgxzOE9wnrZOJw08q14/9tLixaxjjKPSlX1jAs7vJ625hSdqEfkJMo1K8OLHG2TsdgTSEOaipM",
        );
    }
}
