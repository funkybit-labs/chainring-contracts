use thiserror::Error;

pub const SECP256K1_SIGNATURE_LENGTH: usize = 64;
pub const SECP256K1_PUBLIC_KEY_LENGTH: usize = 64;
pub const HASH_BYTES: usize = 32;
pub const SUCCESS: u64 = 0;

#[derive(Clone, Copy, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct Secp256k1Pubkey(pub [u8; SECP256K1_PUBLIC_KEY_LENGTH]);

impl Secp256k1Pubkey {
    pub fn new(pubkey_vec: &[u8]) -> Self {
        Self(
            <[u8; SECP256K1_PUBLIC_KEY_LENGTH]>::try_from(<&[u8]>::clone(&pubkey_vec))
                .expect("Slice must be the same length as a Pubkey"),
        )
    }

    pub fn to_bytes(self) -> [u8; 64] {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum Secp256k1RecoverError {
    #[error("The hash provided to a secp256k1_recover is invalid")]
    InvalidHash,
    #[error("The recovery_id provided to a secp256k1_recover is invalid")]
    InvalidRecoveryId,
    #[error("The signature provided to a secp256k1_recover is invalid")]
    InvalidSignature,
}

impl From<u64> for Secp256k1RecoverError {
    fn from(v: u64) -> Secp256k1RecoverError {
        match v {
            1 => Secp256k1RecoverError::InvalidHash,
            2 => Secp256k1RecoverError::InvalidRecoveryId,
            3 => Secp256k1RecoverError::InvalidSignature,
            _ => panic!("Unsupported Secp256k1RecoverError"),
        }
    }
}

impl From<Secp256k1RecoverError> for u64 {
    fn from(v: Secp256k1RecoverError) -> u64 {
        match v {
            Secp256k1RecoverError::InvalidHash => 1,
            Secp256k1RecoverError::InvalidRecoveryId => 2,
            Secp256k1RecoverError::InvalidSignature => 3,
        }
    }
}

pub fn secp256k1_recover(
    hash: &[u8],
    recovery_id: u8,
    signature: &[u8],
) -> Result<Secp256k1Pubkey, Secp256k1RecoverError> {
    let mut pubkey_buffer = [0u8; SECP256K1_PUBLIC_KEY_LENGTH];

    #[cfg(target_os = "solana")]
    {
        let result = unsafe {
            crate::syscalls::sol_secp256k1_recover(
                hash.as_ptr(),
                recovery_id as u64,
                signature.as_ptr(),
                pubkey_buffer.as_mut_ptr(),
            )
        };

        match result {
            crate::entrypoint::SUCCESS => Ok(Secp256k1Pubkey::new(&pubkey_buffer)),
            _ => Err(result.into()),
        }
    }

    #[cfg(not(target_os = "solana"))]
    {
        let result = crate::program_stubs::sol_secp256k1_recover(
            hash.as_ptr(),
            recovery_id as u64,
            signature.as_ptr(),
            pubkey_buffer.as_mut_ptr(),
        );
        match result {
            crate::entrypoint::SUCCESS => Ok(Secp256k1Pubkey::new(&pubkey_buffer)),
            _ => Err(result.into()),
        }
    }
}
