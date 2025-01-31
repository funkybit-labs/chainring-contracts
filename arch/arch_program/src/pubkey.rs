use borsh::{BorshDeserialize, BorshSerialize};
use bytemuck::{Pod, Zeroable};
use serde::{Deserialize, Serialize};

/// Number of bytes in a pubkey
pub const PUBKEY_BYTES: usize = 32;

#[repr(C)]
#[derive(
    Clone,
    Eq,
    PartialEq,
    Hash,
    PartialOrd,
    Ord,
    Default,
    Copy,
    Serialize,
    Deserialize,
    BorshSerialize,
    BorshDeserialize,
    Pod,
    Zeroable,
)]
pub struct Pubkey(pub [u8; 32]);

impl Pubkey {
    pub fn serialize(&self) -> [u8; 32] {
        self.0
    }

    pub fn from_slice(data: &[u8]) -> Self {
        let mut tmp = [0u8; 32];
        tmp[..data.len()].copy_from_slice(data);
        Self(tmp)
    }

    pub fn system_program() -> Self {
        let mut tmp = [0u8; 32];
        tmp[31] = 1;
        Self(tmp)
    }

    pub fn is_system_program(&self) -> bool {
        let mut tmp = [0u8; 32];
        tmp[31] = 1;
        self.0 == tmp
    }

    /// unique Pubkey for tests and benchmarks.
    pub fn new_unique() -> Self {
        use crate::atomic_u64::AtomicU64;
        static I: AtomicU64 = AtomicU64::new(1);

        let mut b = [0u8; 32];
        let i = I.fetch_add(1);
        // use big endian representation to ensure that recent unique pubkeys
        // are always greater than less recent unique pubkeys
        b[0..8].copy_from_slice(&i.to_be_bytes());
        Self::from(b)
    }

    /// Log a `Pubkey` from a program
    pub fn log(&self) {
        unsafe { crate::syscalls::sol_log_pubkey(self.as_ref() as *const _ as *const u8) };
    }

    #[cfg(not(target_os = "solana"))]
    pub fn is_on_curve(pubkey: &[u8]) -> bool {
        match bitcoin::secp256k1::PublicKey::from_slice(pubkey) {
            Ok(_) => true,
            Err(_) => false,
        }
    }

    pub fn find_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> (Pubkey, u8) {
        Self::try_find_program_address(seeds, program_id)
            .unwrap_or_else(|| panic!("Unable to find a viable program address bump seed"))
    }

    pub fn try_find_program_address(seeds: &[&[u8]], program_id: &Pubkey) -> Option<(Pubkey, u8)> {
        // Perform the calculation inline, calling this from within a program is
        // not supported
        #[cfg(not(target_os = "solana"))]
        {
            let mut bump_seed = [std::u8::MAX];
            for _ in 0..std::u8::MAX {
                {
                    let mut seeds_with_bump = seeds.to_vec();
                    seeds_with_bump.push(&bump_seed);
                    match Self::create_program_address(&seeds_with_bump, program_id) {
                        Ok(address) => return Some((address, bump_seed[0])),
                        Err(ProgramError::InvalidSeeds) => (),
                        e => {
                            println!("error {:?}", e);
                            break;
                        }
                    }
                }
                bump_seed[0] -= 1;
            }
            None
        }
        // Call via a system call to perform the calculation
        #[cfg(target_os = "solana")]
        {
            let mut bytes = [0; 32];
            let mut bump_seed = std::u8::MAX;
            let result = unsafe {
                crate::syscalls::sol_try_find_program_address(
                    seeds as *const _ as *const u8,
                    seeds.len() as u64,
                    program_id as *const _ as *const u8,
                    &mut bytes as *mut _ as *mut u8,
                    &mut bump_seed as *mut _ as *mut u8,
                )
            };
            match result {
                crate::entrypoint::SUCCESS => Some((Pubkey::from(bytes), bump_seed)),
                _ => None,
            }
        }
    }

    pub fn create_program_address(
        seeds: &[&[u8]],
        program_id: &Pubkey,
    ) -> Result<Pubkey, ProgramError> {
        if seeds.len() > MAX_SEEDS {
            println!("seeds.len() {} > MAX_SEEDS {}", seeds.len(), MAX_SEEDS);
            return Err(ProgramError::MaxSeedLengthExceeded);
        }
        for seed in seeds.iter() {
            if seed.len() > MAX_SEED_LEN {
                println!("seed.len() {} > MAX_SEED_LEN {}", seed.len(), MAX_SEED_LEN);
                return Err(ProgramError::MaxSeedLengthExceeded);
            }
        }

        // Perform the calculation inline, calling this from within a program is
        // not supported
        #[cfg(not(target_os = "solana"))]
        {
            let mut hash = vec![];
            for seed in seeds.iter() {
                hash.extend_from_slice(seed);
            }
            hash.extend_from_slice(program_id.as_ref());
            let hash = hex::decode(sha256::digest(&hash))?;

            if Self::is_on_curve(&hash) {
                return Err(ProgramError::InvalidSeeds);
            }

            Ok(Self::from_slice(&hash))
        }
        // Call via a system call to perform the calculation
        #[cfg(target_os = "solana")]
        {
            let mut bytes = [0; 32];
            let result = unsafe {
                crate::syscalls::sol_create_program_address(
                    seeds as *const _ as *const u8,
                    seeds.len() as u64,
                    program_id as *const _ as *const u8,
                    &mut bytes as *mut _ as *mut u8,
                )
            };
            match result {
                crate::entrypoint::SUCCESS => Ok(Self::from_slice(&bytes)),
                _ => Err(result.into()),
            }
        }
    }
}

pub const MAX_SEEDS: usize = 16;
pub const MAX_SEED_LEN: usize = 32;

impl std::fmt::LowerHex for Pubkey {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        let ser = self.serialize();
        for ch in &ser[..] {
            write!(f, "{:02x}", *ch)?;
        }
        Ok(())
    }
}

use core::fmt;

use crate::program_error::ProgramError;

/// TODO:
///  Change this in future according to the correct base implementation
impl fmt::Display for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{:?}", self.0)
    }
}

impl fmt::Debug for Pubkey {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", hex::encode(self.0))
    }
}

impl AsRef<[u8]> for Pubkey {
    fn as_ref(&self) -> &[u8] {
        &self.0[..]
    }
}

impl AsMut<[u8]> for Pubkey {
    fn as_mut(&mut self) -> &mut [u8] {
        &mut self.0[..]
    }
}

impl From<[u8; 32]> for Pubkey {
    fn from(value: [u8; 32]) -> Self {
        Pubkey(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pubkey::Pubkey;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn fuzz_serialize_deserialize_pubkey(data in any::<[u8; 32]>()) {
            let pubkey = Pubkey::from(data);
            let serialized = pubkey.serialize();
            let deserialized = Pubkey::from_slice(&serialized);
            assert_eq!(pubkey, deserialized);
        }
    }

    #[test]
    fn test_create_program_address() {
        let program_id = Pubkey::new_unique();

        // Test empty seeds
        let result = Pubkey::create_program_address(&[], &program_id);
        assert!(result.is_ok());

        // Test with valid seeds
        let seed1 = b"hello";
        let seed2 = b"world";
        let result = Pubkey::create_program_address(&[seed1, seed2], &program_id);
        assert!(result.is_ok());

        // Test exceeding MAX_SEEDS
        let too_many_seeds = vec![&[0u8; 1][..]; MAX_SEEDS + 1];
        let result = Pubkey::create_program_address(&too_many_seeds[..], &program_id);
        assert_eq!(result.unwrap_err(), ProgramError::MaxSeedLengthExceeded);

        // Test exceeding MAX_SEED_LEN
        let long_seed = &[0u8; MAX_SEED_LEN + 1];
        let result = Pubkey::create_program_address(&[long_seed], &program_id);
        assert_eq!(result.unwrap_err(), ProgramError::MaxSeedLengthExceeded);
    }

    #[test]
    fn test_find_program_address() {
        let program_id = Pubkey::new_unique();
        let seed1: &[u8] = b"hello";

        // Test basic functionality
        let (address, bump) = Pubkey::find_program_address(&[seed1], &program_id);
        assert!(bump <= std::u8::MAX);

        // Verify that the found address is valid
        let mut seeds_with_bump = vec![seed1];
        let bump_array = [bump];
        seeds_with_bump.push(&bump_array);
        let created_address =
            Pubkey::create_program_address(&seeds_with_bump, &program_id).unwrap();
        assert_eq!(address, created_address);
    }

    #[test]
    fn test_try_find_program_address() {
        let program_id = Pubkey::new_unique();
        let seed1: &[u8] = b"hello";

        // Test basic functionality
        let result = Pubkey::try_find_program_address(&[seed1], &program_id);
        assert!(result.is_some());

        let (address, bump) = result.unwrap();
        assert!(bump <= std::u8::MAX);

        // Verify that the found address is valid
        let mut seeds_with_bump = vec![seed1];
        let bump_array = [bump];
        seeds_with_bump.push(&bump_array);
        let created_address =
            Pubkey::create_program_address(&seeds_with_bump, &program_id).unwrap();
        assert_eq!(address, created_address);
    }
}
