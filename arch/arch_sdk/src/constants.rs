//! This module contains constants
/// The file path where the caller stores information
pub const CALLER_FILE_PATH: &str = ".caller.json";
pub const PROGRAM_FILE_PATH: &str = ".program.json";

/// Local address for node 1
pub const NODE1_ADDRESS: &str = "http://127.0.0.1:9002/";

/// Arbitrary example names for HelloWorld program
pub const NAME1: &str = "Amine";
pub const NAME2: &str = "Marouane";

/// RPC methods
pub const ASSIGN_AUTHORITY: &str = "assign_authority";
pub const READ_ACCOUNT_INFO: &str = "read_account_info";
pub const DEPLOY_PROGRAM: &str = "deploy_program";
pub const SEND_TRANSACTION: &str = "send_transaction";
pub const GET_PROGRAM: &str = "get_program";
pub const GET_BLOCK: &str = "get_block";
pub const GET_BLOCK_COUNT: &str = "get_block_count";
pub const GET_BEST_BLOCK_HASH: &str = "get_best_block_hash";
pub const GET_PROCESSED_TRANSACTION: &str = "get_processed_transaction";
pub const GET_ACCOUNT_ADDRESS: &str = "get_account_address";

/// Data
pub const BITCOIN_NODE_ENDPOINT: &str = "http://127.0.0.1:18443/wallet/testwallet";
pub const BITCOIN_NODE_USERNAME: &str = "user";
pub const BITCOIN_NODE_PASSWORD: &str = "password";
pub const BITCOIN_NETWORK: bitcoin::Network = bitcoin::Network::Regtest;
pub const MINING_ADDRESS: &str = "bcrt1q9s6pf9hswah20jjnzmyvk9s2xwp7srz6m2r5tw";

pub const BITCOIN_NODE1_ADDRESS: &str = "http://127.0.0.1:18443/wallet/testwallet";
pub const BITCOIN_NODE2_ADDRESS: &str = "http://127.0.0.1:18453/wallet/testwallet";

pub const BITCOIN_NODE1_P2P_ADDRESS: &str = "127.0.0.1:18444";
pub const BITCOIN_NODE2_P2P_ADDRESS: &str = "127.0.0.1:18454";

/// Hack for Error codes
pub const TRANSACTION_NOT_FOUND_CODE: i64 = 404;
