pub mod sig;
pub mod address;
mod bip322;

use chrono::{DateTime, Utc};
use sha2::{Sha256, Digest};
use ripemd::Ripemd160;

const SATOSHIS_PER_BITCOIN: f64 = 100_000_000.0;

fn satoshi_to_btc(satoshis: u64) -> f64 {
    satoshis as f64 / SATOSHIS_PER_BITCOIN
}

pub fn create_bitcoin_withdrawal_message(
    amount: u64,
    symbol: &str,
    bitcoin_address: &str,
    nonce: i64
) -> Vec<u8> {
    let timestamp = DateTime::<Utc>::from_timestamp(nonce / 1000, 0)
        .unwrap()
        .to_rfc3339();

    let message = format!(
        "[funkybit] Please sign this message to authorize withdrawal of {} {} from the exchange to your wallet.\nAddress: {}, Timestamp: {}",
        satoshi_to_btc(amount),
        symbol,
        bitcoin_address,
        timestamp
    );

    message.into_bytes()
}

fn sha256_ripemd160(data: &[u8]) -> [u8; 20] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let sha256 = hasher.finalize();
    let mut hasher = Ripemd160::new();
    hasher.update(sha256);
    hasher.finalize().into()
}

pub fn sha256(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

pub fn double_sha256(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    let first_hash = hasher.finalize();

    let mut hasher = Sha256::new();
    hasher.update(first_hash);
    hasher.finalize().to_vec()
}

pub fn parse_recovery_id(recover_id: u8) -> Result<(u8, bool), String> {
    let shifted = match recover_id {
        h if h > 42 => Err(format!("Header byte too high: {}", h)),
        h if h < 27 => Err(format!("Header byte too low: {}", h)),
        39..=42 => Ok(recover_id - 39),
        35..=38 => Ok(recover_id - 35),
        31..=34 => Ok(recover_id - 31),
        27..=30 => Ok(recover_id - 27),
        _ => unreachable!(),
    };
    if let Ok(shifted) = shifted {
        Ok((shifted, shifted % 2 == 1))
    } else {
        Err(shifted.unwrap_err())
    }
}