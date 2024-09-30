use bech32;
use bech32::{Bech32, ByteIterExt, Fe32, Fe32IterExt, Hrp};

#[derive(Debug, PartialEq)]
pub enum AddressType {
    P2PKH,
    P2SH,
    P2WPKH,
    P2TR,
}

#[derive(Debug, PartialEq)]
pub enum Network {
    Mainnet,
    Testnet,
}

#[derive(Debug)]
pub struct BitcoinAddress {
    pub address: String,
    pub address_type: AddressType,
    pub network: Network,
}

impl BitcoinAddress {
    pub fn new(address: &str) -> Result<Self, &'static str> {
        let (address_type, network) = match address.chars().next() {
            Some('1') => (AddressType::P2PKH, Network::Mainnet),
            Some('3') => (AddressType::P2SH, Network::Mainnet),
            Some('2') => (AddressType::P2SH, Network::Testnet),
            Some('m') | Some('n') => (AddressType::P2PKH, Network::Testnet),
            Some('b') => {
                if address.starts_with("bc1q") {
                    (AddressType::P2WPKH, Network::Mainnet)
                } else if address.starts_with("bc1p") {
                    (AddressType::P2TR, Network::Mainnet)
                } else if address.starts_with("tb1q") || address.starts_with("bcrt1q") {
                    (AddressType::P2WPKH, Network::Testnet)
                } else if address.starts_with("tb1p") || address.starts_with("bcrt1p") {
                    (AddressType::P2TR, Network::Testnet)
                } else {
                    return Err("Invalid Bech32 address");
                }
            },
            _ => return Err("Invalid address format"),
        };

        Ok(BitcoinAddress {
            address: address.to_string(),
            address_type,
            network,
        })
    }
}

pub fn pubkey_hash_to_segwit_address(pubkey_hash: &[u8], expected_address: &BitcoinAddress) -> Result<String, &'static str> {
    let hrp = match expected_address.address.find('1') {
        Some(index) => Hrp::parse(&expected_address.address[..index]),
        None => return Err("Invalid HRP")
    }.unwrap();
    Ok(pubkey_hash
        .iter()
        .copied()
        .bytes_to_fes()
        .with_checksum::<Bech32>(&hrp)
        .with_witness_version(Fe32::Q)
        .chars().collect())
}