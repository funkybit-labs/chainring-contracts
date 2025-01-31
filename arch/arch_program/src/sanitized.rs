use crate::pubkey::Pubkey;

#[derive(Debug, Clone)]
pub struct SanitizedMessage {
    pub message: ArchMessage,
    /// List of boolean with same length as account_keys(), each boolean value indicates if
    /// corresponding account key is writable or not.
    pub is_writable_account_cache: Vec<bool>,
}

impl SanitizedMessage {
    pub fn new(message: ArchMessage) -> Self {
        let is_writable_account_cache = message
            .account_keys
            .iter()
            .enumerate()
            .map(|(i, _key)| message.is_writable_index(i))
            .collect::<Vec<_>>();
        Self {
            message,
            is_writable_account_cache,
        }
    }

    pub fn is_signer(&self, index: usize) -> bool {
        self.message.is_signer(index)
    }

    pub fn is_writable(&self, index: usize) -> bool {
        *self.is_writable_account_cache.get(index).unwrap_or(&false)
    }

    pub fn instructions(&self) -> &Vec<SanitizedInstruction> {
        &self.message.instructions
    }
}

#[derive(Debug, Clone)]
pub struct ArchMessage {
    pub header: MessageHeader,
    pub account_keys: Vec<Pubkey>,
    pub instructions: Vec<SanitizedInstruction>,
}
impl ArchMessage {
    /// Returns true if the account at the specified index was requested to be
    /// writable. This method should not be used directly.
    pub(super) fn is_writable_index(&self, i: usize) -> bool {
        i < (self.header.num_required_signatures - self.header.num_readonly_signed_accounts)
            as usize
            || (i >= self.header.num_required_signatures as usize
                && i < self.account_keys.len()
                    - self.header.num_readonly_unsigned_accounts as usize)
    }

    pub fn header(&self) -> &MessageHeader {
        &self.header
    }

    pub fn is_signer(&self, index: usize) -> bool {
        index < usize::from(self.header().num_required_signatures)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SanitizedInstruction {
    pub program_id: Pubkey,
    pub accounts: Vec<u16>,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct MessageHeader {
    /// The number of signatures required for this message to be considered
    /// valid
    pub num_required_signatures: u8,

    /// The last `num_readonly_signed_accounts` of the signed keys are read-only
    /// accounts.
    pub num_readonly_signed_accounts: u8,

    /// The last `num_readonly_unsigned_accounts` of the unsigned keys are
    /// read-only accounts.
    pub num_readonly_unsigned_accounts: u8,
}
