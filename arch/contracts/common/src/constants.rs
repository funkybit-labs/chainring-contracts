//! This module contains constants

/// The file path where the caller stores information
pub const CALLER_FILE_PATH: &str = "../arch-data/caller.json";
pub const SUBMITTER_FILE_PATH: &str = "../arch-data/submitter.json";

/// Local address for node 1
pub const NODE1_ADDRESS: &str = "http://127.0.0.1:9001/";

/// Arbitrary example names for HelloWorld program
pub const NAME1: &str = "Amine";
pub const NAME2: &str = "Marouane";

/// RPC methods
pub const ASSIGN_AUTHORITY: &str = "assign_authority";
pub const READ_UTXO: &str = "read_utxo";
pub const DEPLOY_PROGRAM: &str = "deploy_program";
pub const SEND_TRANSACTION: &str = "send_transaction";
pub const GET_PROGRAM: &str = "get_program";
pub const GET_BLOCK: &str = "get_block";
pub const GET_BEST_BLOCK_HASH: &str = "get_best_block_hash";
pub const GET_PROCESSED_TRANSACTION: &str = "get_processed_transaction";
pub const GET_CONTRACT_ADDRESS: &str = "get_contract_address";

/// Data

pub const BITCOIN_NODE_ENDPOINT: &str =
    "https://localhost:18443/wallet/testwallet";
pub const BITCOIN_NODE_USERNAME: &str = "user";
pub const BITCOIN_NODE_PASSWORD: &str = "password";


/// Hack for Error codes
pub const TRANSACTION_NOT_FOUND_CODE: i64 = 404;

pub const FAUCET_ADDR: &str = "bcrt1q3nyukkpkg6yj0y5tj6nj80dh67m30p963mzxy7";