use crate::account::AccountMeta;
use crate::instruction::Instruction;
use crate::pubkey::Pubkey;
use crate::utxo::UtxoMeta;

pub fn create_account(txid: [u8; 32], vout: u32, pubkey: Pubkey) -> Instruction {
    Instruction {
        program_id: Pubkey::system_program(),
        accounts: vec![AccountMeta {
            pubkey,
            is_signer: true,
            is_writable: true,
        }],
        data: [&[0][..], &UtxoMeta::from(txid, vout).serialize()].concat(),
    }
}

pub fn write_bytes(offset: u32, len: u32, data: Vec<u8>, pubkey: Pubkey) -> Instruction {
    Instruction {
        program_id: Pubkey::system_program(),
        accounts: vec![AccountMeta {
            pubkey,
            is_signer: true,
            is_writable: true,
        }],
        data: [
            &[1][..],
            offset.to_le_bytes().as_slice(),
            len.to_le_bytes().as_slice(),
            data.as_slice(),
        ]
        .concat(),
    }
}

pub fn deploy(pubkey: Pubkey) -> Instruction {
    Instruction {
        program_id: Pubkey::system_program(),
        accounts: vec![AccountMeta {
            pubkey,
            is_signer: true,
            is_writable: true,
        }],
        data: vec![2],
    }
}

pub fn assign(pubkey: Pubkey, owner: Pubkey) -> Instruction {
    Instruction {
        program_id: Pubkey::system_program(),
        accounts: vec![AccountMeta {
            pubkey,
            is_signer: true,
            is_writable: true,
        }],
        data: [&[3][..], owner.serialize().as_slice()].concat(),
    }
}

pub fn retract(pubkey: Pubkey) -> Instruction {
    Instruction {
        program_id: Pubkey::system_program(),
        accounts: vec![AccountMeta {
            pubkey,
            is_signer: true,
            is_writable: true,
        }],
        data: vec![4],
    }
}

pub fn truncate(pubkey: Pubkey, new_size: u32) -> Instruction {
    Instruction {
        program_id: Pubkey::system_program(),
        accounts: vec![AccountMeta {
            pubkey,
            is_signer: true,
            is_writable: true,
        }],
        data: [&[5][..], new_size.to_le_bytes().as_slice()].concat(),
    }
}
