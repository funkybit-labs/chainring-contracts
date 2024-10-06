/// Running Tests
///
#[cfg(test)]
mod tests {
    use common::constants::*;
    use arch_program::{pubkey::Pubkey, system_instruction::SystemInstruction, instruction::Instruction, account::AccountMeta};
    use common::helper::*;
    use borsh::{BorshSerialize, BorshDeserialize};
    use std::{fmt, fs};
    use std::str::FromStr;
    use bitcoin::key::UntweakedKeypair;
    use bitcoin::{Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, Txid, TxIn, Witness};
    use bitcoin::absolute::LockTime;
    use bitcoin::transaction::Version;
    use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};

    fn cleanup_account_keys() {
        for file in vec![WALLET1_FILE_PATH, WALLET2_FILE_PATH, SUBMITTER_FILE_PATH, FEE_ACCOUNT_FILE_PATH] {
            delete_secret_file(file);
        }
        for file in TOKEN_FILE_PATHS {
            delete_secret_file(file);
        }
        let _ = SETUP.program_pubkey;
    }

    use env_logger;
    use log::{debug, warn};
    use sha256::digest;
    use common::models::CallerInfo;

    const TOKEN_FILE_PATHS: &'static [&'static str] = &["../../data/token1.json", "../../data/token2.json"];
    pub const SUBMITTER_FILE_PATH: &str = "../../data/submitter.json";
    pub const WALLET1_FILE_PATH: &str = "../../data/wallet1.json";
    pub const WALLET2_FILE_PATH: &str = "../../data/wallet2.json";
    pub const FEE_ACCOUNT_FILE_PATH: &str = "../../data/fee_account.json";

    impl fmt::Display for Balance {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{} {}", self.address, self.balance)
        }
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize, PartialEq, Debug)]
    pub enum NetworkType {
        /// Mainnet Bitcoin.
        Bitcoin,
        /// Bitcoin's testnet network.
        Testnet,
        /// Bitcoin's signet network.
        Signet,
        /// Bitcoin's regtest network.
        Regtest,
    }

    impl NetworkType {
        pub fn to_vec(&self) -> Vec<u8> {
            vec![
                match self {
                    Self::Bitcoin => 0,
                    Self::Testnet => 1,
                    Self::Signet => 2,
                    Self::Regtest => 3
                }
            ]
        }

        pub fn from_u8(data: u8) -> Self {
            match data {
                0 => Self::Bitcoin,
                1 => Self::Testnet,
                2 => Self::Signet,
                3 => Self::Regtest,
                _ => Self::Bitcoin
            }
        }
    }

    #[derive(Clone, Debug)]
    pub struct Balance {
        pub address: String,
        pub balance: u64,
    }

    #[derive(Clone, Debug)]
    pub struct TokenBalances {
        pub version: u32,
        pub program_state_account: Pubkey,
        pub token_id: String,
        pub balances: Vec<Balance>,
    }

    #[derive(Clone, Debug)]
    pub struct ProgramState {
        pub version: u32,
        pub fee_account_address: String,
        pub program_change_address: String,
        pub network_type: NetworkType,
        pub settlement_batch_hash: [u8; 32],
        pub last_settlement_batch_hash: [u8; 32],
        pub last_withdrawal_batch_hash: [u8; 32],
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct Adjustment {
        pub address_index: u32,
        pub amount: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct TokenBalanceSetup {
        pub account_index: u8,
        pub wallet_addresses: Vec<String>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct Withdrawal {
        pub address_index: u32,
        pub amount: u64,
        pub fee_amount: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub enum ProgramInstruction {
        InitProgramState(InitProgramStateParams),
        InitTokenState(InitTokenStateParams),
        InitWalletBalances(InitWalletBalancesParams),
        BatchDeposit(DepositBatchParams),
        BatchWithdraw(WithdrawBatchParams),
        PrepareBatchSettlement(SettlementBatchParams),
        SubmitBatchSettlement(SettlementBatchParams),
        RollbackBatchSettlement(),
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct InitProgramStateParams {
        pub fee_account: String,
        pub program_change_address: String,
        pub network_type: NetworkType,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct InitTokenStateParams {
        pub token_id: String,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct InitWalletBalancesParams {
        pub token_balance_setups: Vec<TokenBalanceSetup>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct DepositBatchParams {
        pub token_deposits: Vec<TokenDeposits>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct TokenDeposits {
        pub account_index: u8,
        pub deposits: Vec<Adjustment>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct TokenWithdrawals {
        pub account_index: u8,
        pub withdrawals: Vec<Withdrawal>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct WithdrawBatchParams {
        pub token_withdrawals: Vec<TokenWithdrawals>,
        pub change_amount: u64,
        pub tx_hex: Vec<u8>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct SettlementAdjustments {
        pub account_index: u8,
        pub increments: Vec<Adjustment>,
        pub decrements: Vec<Adjustment>,
        pub fee_amount: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct SettlementBatchParams {
        pub settlements: Vec<SettlementAdjustments>
    }

    struct Setup {
        program_keypair: UntweakedKeypair,
        program_pubkey: Pubkey
    }
    impl Setup {
        fn init() -> Self {
            env_logger::init();
            delete_secret_file(PROGRAM_FILE_PATH);
            let (program_keypair, program_pubkey) = deploy_program();
            Self {
                program_keypair,
                program_pubkey
            }
        }
    }

    use lazy_static::lazy_static;

    lazy_static! {
        static ref SETUP: Setup = Setup::init();
    }

    const MAX_ADDRESS_LENGTH: usize = 92;
    const MAX_TOKEN_ID_LENGTH: usize = 32;

    const BALANCE_SIZE: usize  = MAX_ADDRESS_LENGTH + 8;

    const OFFSET_NETWORK_TYPE: usize = 4 + MAX_ADDRESS_LENGTH * 2;
    const OFFSET_SETTLEMENT_HASH: usize  = 4 + 1 + MAX_ADDRESS_LENGTH * 2;
    const OFFSET_LAST_SETTLEMENT_HASH: usize = 4 + 1 + 32 + MAX_ADDRESS_LENGTH * 2;
    const OFFSET_LAST_WITHDRAWAL_HASH: usize = 4 + 1 + 32 + 32 + MAX_ADDRESS_LENGTH * 2;

    impl TokenBalances {
        pub fn to_vec(&self) -> Vec<u8> {
            let mut serialized = vec![];
            serialized.extend(self.version.to_le_bytes());
            serialized.extend(self.program_state_account.serialize());
            let mut tmp = [0u8; MAX_TOKEN_ID_LENGTH];
            let bytes = self.token_id.as_bytes();
            tmp[..bytes.len()].copy_from_slice(bytes);
            serialized.extend(tmp.as_slice());
            serialized.extend((self.balances.len() as u32).to_le_bytes());
            for balance in self.balances.iter() {
                serialized.extend_from_slice(&balance.to_vec());
            }
            serialized
        }

        pub fn from_slice(data: &[u8]) -> Self {
            let mut tmp = [0u8; MAX_TOKEN_ID_LENGTH];
            let mut offset = MAX_TOKEN_ID_LENGTH + 36;
            tmp[..MAX_TOKEN_ID_LENGTH].copy_from_slice(&data[36..offset]);
            let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_TOKEN_ID_LENGTH);
            let mut token_balances = TokenBalances {
                version: u32::from_le_bytes(
                    data[0..4]
                        .try_into()
                        .expect("slice with incorrect length"),
                ),
                program_state_account: Pubkey::from_slice(
                    data[4..36]
                      .try_into()
                      .expect("slice with incorrect length")
                ),
                token_id: String::from_utf8(tmp[..pos].to_vec()).unwrap(),
                balances: vec![],
            };

            let inputs_to_sign_length: usize = u32::from_le_bytes(
                data[offset..offset+4]
                    .try_into()
                    .expect("slice with incorrect length"),
            ) as usize;

            offset += 4;
            for _ in 0..inputs_to_sign_length {
                token_balances
                    .balances
                    .push(Balance::from_slice(
                        data[offset..offset + BALANCE_SIZE].try_into().expect("slice with incorrect length")
                    ));
                offset += BALANCE_SIZE;
            }
            token_balances
        }
    }

    impl Balance {
        pub fn to_vec(&self) -> Vec<u8> {
            let mut serialized = vec![];
            let mut tmp = [0u8; MAX_ADDRESS_LENGTH];
            let bytes = self.address.as_bytes();
            tmp[..bytes.len()].copy_from_slice(bytes);

            serialized.extend(tmp.as_slice());
            serialized.extend(self.balance.to_le_bytes());

            serialized
        }

        pub fn from_slice(data: &[u8]) -> Self {
            let mut tmp = [0u8; MAX_ADDRESS_LENGTH];
            tmp[..MAX_ADDRESS_LENGTH].copy_from_slice(&data[..MAX_ADDRESS_LENGTH]);
            let pos = tmp.iter().position(|&r| r == 0).unwrap_or(MAX_ADDRESS_LENGTH);
            Balance {
                address: String::from_utf8(tmp[..pos].to_vec()).unwrap(),
                balance: u64::from_le_bytes(
                    data[MAX_ADDRESS_LENGTH..MAX_ADDRESS_LENGTH+8]
                        .try_into()
                        .expect("slice with incorrect length"),
                )
            }
        }
    }

    impl ProgramState {
        pub fn to_vec(&self) -> Vec<u8> {
            let mut serialized = vec![];
            serialized.extend(self.version.to_le_bytes());
            let mut tmp = [0u8; MAX_ADDRESS_LENGTH];
            let bytes = self.fee_account_address.as_bytes();
            tmp[..bytes.len()].copy_from_slice(self.fee_account_address.as_bytes());
            serialized.extend(tmp.as_slice());
            let bytes = self.program_change_address.as_bytes();
            tmp[..bytes.len()].copy_from_slice(bytes);
            serialized.extend(tmp.as_slice());
            serialized.extend(self.network_type.to_vec());
            serialized.extend(self.settlement_batch_hash.as_slice());
            serialized.extend(self.last_settlement_batch_hash.as_slice());
            serialized.extend(self.last_withdrawal_batch_hash.as_slice());
            serialized
        }

        fn hash_from_slice(data: &[u8], offset: usize) -> [u8; 32] {
            let mut tmp = [0u8; 32];
            tmp[..32].copy_from_slice(data[offset..offset+32]
                .try_into()
                .expect("slice with incorrect length"));
            tmp
        }

        fn address_from_slice(data: &[u8], offset: usize) -> String {
            let pos = data[offset..offset+MAX_ADDRESS_LENGTH].iter().position(|&r| r == 0).unwrap_or(MAX_TOKEN_ID_LENGTH);
            String::from_utf8(data[offset..offset+pos].to_vec()).unwrap()
        }

        pub fn from_slice(data: &[u8]) -> Self {
            ProgramState {
                version: u32::from_le_bytes(
                    data[0..4]
                        .try_into()
                        .expect("slice with incorrect length"),
                ),
                fee_account_address: Self::address_from_slice(data, 4),
                program_change_address: Self::address_from_slice(data, MAX_ADDRESS_LENGTH + 4),
                network_type: NetworkType::from_u8(data[OFFSET_NETWORK_TYPE]),
                settlement_batch_hash: Self::hash_from_slice(data, OFFSET_SETTLEMENT_HASH),
                last_settlement_batch_hash: Self::hash_from_slice(data, OFFSET_LAST_SETTLEMENT_HASH),
                last_withdrawal_batch_hash: Self::hash_from_slice(data, OFFSET_LAST_WITHDRAWAL_HASH)
            }
        }
    }

    #[test]
    fn test_deposit_and_withdrawal() {
        cleanup_account_keys();
        let accounts = onboard_state_accounts(vec!["btc"]);

        let token_account = accounts[1].clone();

        let wallet = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();

        deposit(
            wallet.address.to_string().clone(),
            "btc",
            token_account.clone(),
            10000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet.address.to_string().clone(),
                    balance: 10000,
                }
            ]
        );

        let address = get_account_address(SETUP.program_pubkey);
        println!("address from rpc is {}", address);
        let program_address = Address::from_str(&address)
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();

        let (txid, vout) = deposit_to_program(10000, &program_address);

        deposit(
            wallet.address.to_string().clone(),
            "btc",
            token_account.clone(),
            6000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet.address.to_string().clone(),
                    balance: 16000,
                }
            ]
        );


        let (withdraw_tx, change_amount) = prepare_withdrawal(
            5000,
            1500,
            &txid,
            vout
        );

        // perform withdrawal
        let input = WithdrawBatchParams {
            token_withdrawals: vec![TokenWithdrawals {
                account_index: 1,
                withdrawals: vec![Withdrawal {
                    address_index: 1,
                    amount: 5500,
                    fee_amount: 500,
                }],
            }],
            change_amount,
            tx_hex: hex::decode(withdraw_tx).unwrap()
        };
        let expected = TokenBalances {
            version: 0,
            program_state_account: accounts[0],
            token_id: "btc".to_string(),
            balances: vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 500,
                },
                Balance {
                    address: wallet.address.to_string().clone(),
                    balance: 10500,
                }
            ],
        };
        assert_send_and_sign_withdrawal(
            token_account,
            input,
            expected,
            3500
        );
    }

    #[test]
    fn test_settlement_submission() {
        cleanup_account_keys();
        let token1 = "btc";
        let token2 = "rune1";
        let accounts = onboard_state_accounts(vec![token1, token2]);

        let token1_account = accounts[1].clone();
        let token2_account = accounts[2].clone();

        let wallet1 = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let wallet2 = CallerInfo::with_secret_key_file(WALLET2_FILE_PATH).unwrap();
        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();

        deposit(
            wallet1.address.to_string().clone(),
            token1,
            token1_account.clone(),
            10000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet1.address.to_string().clone(),
                    balance: 10000,
                }
            ]
        );

        deposit(
            wallet2.address.to_string().clone(),
            token2,
            token2_account.clone(),
            8000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet2.address.to_string().clone(),
                    balance: 8000,
                }
            ]
        );

        // prepare a settlement
        let input = SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet2.address.to_string(), token1_account),
                            amount: 4500,
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet1.address.to_string(), token1_account),
                            amount: 5000,
                        }
                    ],
                    fee_amount: 500,
                },
                SettlementAdjustments {
                    account_index: 2,
                    increments: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet1.address.to_string(), token2_account),
                            amount: 1000,
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet2.address.to_string(), token2_account),
                            amount: 1000,
                        }
                    ],
                    fee_amount: 0,
                }
            ],
        };

        // prepare settlement
        assert_send_and_sign_prepare_settlement(
            accounts.clone(),
            input.clone()
        );


        // now submit the settlement
        assert_send_and_sign_submit_settlement(
            accounts.clone(),
            input.clone()
        );

        let token1_account_info = read_account_info(NODE1_ADDRESS, token1_account.clone()).unwrap();

        assert_eq!(
            TokenBalances {
                version: 0,
                program_state_account: accounts[0],
                token_id: token1.to_string(),
                balances: vec![
                    Balance {
                        address: fee_account.address.to_string(),
                        balance: 500,
                    },
                    Balance {
                        address: wallet1.address.to_string(),
                        balance: 5000,
                    },
                    Balance {
                        address: wallet2.address.to_string(),
                        balance: 4500,
                    }
                ],
            }.to_vec(),
            TokenBalances::from_slice(token1_account_info.data.as_slice()).to_vec()
        );

        let token2_account_info = read_account_info(NODE1_ADDRESS, token2_account.clone()).unwrap();

        assert_eq!(
            TokenBalances {
                version: 0,
                program_state_account: accounts[0],
                token_id: token2.to_string(),
                balances: vec![
                    Balance {
                        address: fee_account.address.to_string(),
                        balance: 0,
                    },
                    Balance {
                        address: wallet2.address.to_string(),
                        balance: 7000,
                    },
                    Balance {
                        address: wallet1.address.to_string(),
                        balance: 1000,
                    }
                ],
            }.to_vec(),
            TokenBalances::from_slice(token2_account_info.data.as_slice()).to_vec()
        );


        // start another one and maake sure we can rollback
        assert_send_and_sign_prepare_settlement(
            accounts.clone(),
            input.clone()
        );

        assert_send_and_sign_rollback_settlement();
    }

    #[test]
    fn test_deposits_to_multiple_wallets() {
        cleanup_account_keys();
        let token1 = "btc";
        let accounts = onboard_state_accounts(vec![token1]);

        let token1_account = accounts[1].clone();

        let wallet1 = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let wallet2 = CallerInfo::with_secret_key_file(WALLET2_FILE_PATH).unwrap();
        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();

        deposit(
            wallet1.address.to_string().clone(),
            token1,
            token1_account.clone(),
            10000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet1.address.to_string().clone(),
                    balance: 10000,
                }
            ]
        );

        deposit(
            wallet2.address.to_string().clone(),
            token1,
            token1_account.clone(),
            8000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet1.address.to_string().clone(),
                    balance: 10000,
                },
                Balance {
                    address: wallet2.address.to_string().clone(),
                    balance: 8000,
                }
            ]
        );

        deposit(
            wallet2.address.to_string().clone(),
            token1,
            token1_account.clone(),
            6000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet1.address.to_string().clone(),
                    balance: 10000,
                },
                Balance {
                    address: wallet2.address.to_string().clone(),
                    balance: 14000,
                }
            ]
        );
    }

    #[test]
    fn test_setup_many_wallets() {
        cleanup_account_keys();
        let token1 = "btc";
        let accounts = onboard_state_accounts(vec![token1]);

        let token_account = accounts[1].clone();
        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenBalances = TokenBalances::from_slice(&account_info.data);
        assert_eq!(1, token_balances.balances.len());
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        // create 1000 wallets
        let wallets = (0..1000)
            .map(|_| CallerInfo::generate_new().unwrap().address.to_string())
            .collect::<Vec<String>>();

        // send in chunks of 25 so not to exceed max instruction size
        let txids: Vec<String> = wallets
            .chunks(25)
            .enumerate()
            .map(|(i, chunk)| {
                let params = InitWalletBalancesParams {
                    token_balance_setups: vec![
                        TokenBalanceSetup {
                            account_index: 1,
                            wallet_addresses: chunk.to_vec()
                        }
                    ],
                };

                let (txid, _) = sign_and_send_instruction(
                    Instruction {
                        program_id: SETUP.program_pubkey,
                        accounts: vec![
                            AccountMeta {
                                pubkey: submitter_pubkey,
                                is_signer: true,
                                is_writable: false
                            },
                            AccountMeta {
                                pubkey: token_account,
                                is_signer: false,
                                is_writable: true
                            }
                        ],
                        data: borsh::to_vec(&ProgramInstruction::InitWalletBalances(params.clone())).unwrap()
                    },
                    vec![submitter_keypair],
                ).expect("signing and sending a transaction should not fail");
                debug!("submitted tx {} to arch for {}", txid.clone(), i);
                std::thread::sleep(std::time::Duration::from_millis(200));
                txid
            })
            .collect::<Vec<String>>();

        for (i, txid) in txids.iter().enumerate() {
            match get_processed_transaction(NODE1_ADDRESS, txid.clone()) {
                Ok(_) => debug!("Transaction {} (ID: {}) processed successfully", i + 1, txid),
                Err(e) => warn!("Failed to process transaction {} (ID: {}): {:?}", i + 1, txid, e),
            }
        }

        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenBalances = TokenBalances::from_slice(&account_info.data);
        assert_eq!(wallets.len() + 1, token_balances.balances.len());

        for (i, wallet) in wallets.iter().enumerate() {
            assert_eq!(
                i + 1,
                token_balances.balances.clone().into_iter().position(|r| r.address == *wallet).unwrap()
            )
        }

    }

    // support functions
    fn deposit(
        address: String,
        token: &str,
        token_account: Pubkey,
        amount: u64,
        expected_balances: Vec<Balance>
    ) {
        let (_, submitter_pubkey) = create_new_account(SUBMITTER_FILE_PATH);
        let input = DepositBatchParams {
            token_deposits: vec![
                TokenDeposits {
                    account_index: 1,
                    deposits: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(address.clone(), token_account),
                            amount,
                        }
                    ],
                }
            ],
        };
        let expected = TokenBalances {
            version: 0,
            program_state_account: submitter_pubkey,
            token_id: token.to_string(),
            balances: expected_balances
        };
        assert_send_and_sign_deposit(
            token_account,
            input,
            expected,
        )
    }

    fn onboard_state_accounts(tokens: Vec<&str>) -> Vec<Pubkey> {
        debug!("Performing onboard program state");

        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();

        let mut accounts: Vec<Pubkey> = vec![];
        let (submitter_keypair, submitter_pubkey) = create_new_account(SUBMITTER_FILE_PATH);
        debug!("Created program state account");

        assign_ownership(submitter_keypair, submitter_pubkey, SETUP.program_pubkey.clone());
        debug!("Assigned ownership for program state account");

        let program_change_address = get_account_address(SETUP.program_pubkey);

        init_program_state_account(
            InitProgramStateParams {
                fee_account: fee_account.address.to_string(),
                program_change_address: program_change_address.clone(),
                network_type: NetworkType::Regtest

            },
            ProgramState {
                version: 0,
                fee_account_address: fee_account.address.to_string(),
                program_change_address,
                network_type: NetworkType::Regtest,
                settlement_batch_hash: [0u8; 32],
                last_settlement_batch_hash: [0u8; 32],
                last_withdrawal_batch_hash: [0u8; 32],
            },
        );
        debug!("Initialized program state");
        accounts.push(submitter_pubkey);

        for (index, token) in tokens.iter().enumerate() {
            let (token_keypair, token_pubkey) = create_new_account(TOKEN_FILE_PATHS[index]);
            assign_ownership(token_keypair, token_pubkey, SETUP.program_pubkey.clone());
            debug!("Created and assigned ownership for token state account");
            accounts.push(token_pubkey);
            init_token_state_account(
                InitTokenStateParams {
                    token_id: token.to_string(),
                },
                token_pubkey,
                TokenBalances {
                    version: 0,
                    program_state_account: submitter_pubkey,
                    token_id: token.to_string(),
                    balances: vec![Balance {
                        address: fee_account.address.to_string(),
                        balance: 0,
                    }],
                },
            );
            debug!("Initialized token state account");

        }
        accounts
    }
    //
    fn assert_send_and_sign_deposit(
        token_account: Pubkey,
        params: DepositBatchParams,
        expected: TokenBalances,
    ) {
        debug!("Performing Deposit");
        let (submitter_keypair, submitter_pubkey)  = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: false
                    },
                    AccountMeta {
                       pubkey: token_account,
                       is_signer: false,
                       is_writable: true
                    }
                ],
                data: borsh::to_vec(&ProgramInstruction::BatchDeposit(params.clone())).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let token_account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();

        assert_eq!(
            expected.to_vec(), TokenBalances::from_slice(token_account.data.as_slice()).to_vec()
        );
    }

    fn assert_send_and_sign_withdrawal(
        token_account: Pubkey,
        params: WithdrawBatchParams,
        expected: TokenBalances,
        expected_change_amount: u64
    ) {
        debug!("Performing Withdrawal");
        let (submitter_keypair, submitter_pubkey)  = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let expected = expected.to_vec();
        let wallet  = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let program_change_address = Address::from_str(&get_account_address(SETUP.program_pubkey))
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true
                    },
                    AccountMeta {
                        pubkey: token_account,
                        is_signer: false,
                        is_writable: true
                    }
                ],
                data: borsh::to_vec(&ProgramInstruction::BatchWithdraw(params.clone())).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let token_account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();

        assert_eq!(
            expected.to_vec(), TokenBalances::from_slice(token_account.data.as_slice()).to_vec()
        );

        let bitcoin_txid = Txid::from_str(&processed_tx.bitcoin_txids[0].clone()).unwrap();
        debug!("bitcoin tx is {}", bitcoin_txid);

        let userpass = Auth::UserPass(
            BITCOIN_NODE_USERNAME.to_string(),
            BITCOIN_NODE_PASSWORD.to_string(),
        );
        let rpc =
            Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

        let sent_tx = rpc
            .get_raw_transaction(&bitcoin_txid, None)
            .expect("should get raw transaction");
        let mut wallet_amount: u64 = 0;
        let mut change_amount: u64 = 0;

        for output in sent_tx.output.iter() {
            if output.script_pubkey == wallet.address.script_pubkey() {
                wallet_amount = output.value.to_sat();
            }
            if output.script_pubkey == program_change_address.script_pubkey() {
                change_amount = output.value.to_sat();
            }
        }
        assert_eq!(
            params.token_withdrawals[0].withdrawals[0].amount - params.token_withdrawals[0].withdrawals[0].fee_amount,
            wallet_amount
        );

        assert_eq!(
            expected_change_amount,
            change_amount
        );

        debug!("Wallet amount is {}, Change amount is {}", wallet_amount, change_amount)
    }

    fn assert_send_and_sign_prepare_settlement(
        accounts: Vec<Pubkey>,
        params: SettlementBatchParams,
    ) {
        debug!("Performing prepare Settlement Batch");
        let (submitter_keypair, submitter_pubkey)  = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true
                    },
                    AccountMeta {
                        pubkey: accounts[1],
                        is_signer: false,
                        is_writable: false
                    },
                    AccountMeta {
                        pubkey: accounts[2],
                        is_signer: false,
                        is_writable: false
                    }
                ],
                data: borsh::to_vec(&ProgramInstruction::PrepareBatchSettlement(params.clone())).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        let program_state: ProgramState = ProgramState::from_slice(&state_account.data);
        assert_eq!(
            hex::encode(program_state.settlement_batch_hash),
            hash(borsh::to_vec(&params).unwrap()),
        );
    }

    fn assert_send_and_sign_rollback_settlement() {
        debug!("Performing rollback Settlement Batch");
        let (submitter_keypair, submitter_pubkey)  = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true
                    },
                ],
                data: borsh::to_vec(&ProgramInstruction::RollbackBatchSettlement()).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        let program_state: ProgramState = ProgramState::from_slice(&state_account.data);
        assert_eq!(
            program_state.settlement_batch_hash, [0u8; 32]
        );
    }


    fn assert_send_and_sign_submit_settlement(
        accounts: Vec<Pubkey>,
        params: SettlementBatchParams,
    ) {
        debug!("Performing submit Settlement Batch");
        let (submitter_keypair, submitter_pubkey)  = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true
                    },
                    AccountMeta {
                        pubkey: accounts[1],
                        is_signer: false,
                        is_writable: true
                    },
                    AccountMeta {
                        pubkey: accounts[2],
                        is_signer: false,
                        is_writable: true
                    }
                ],
                data: borsh::to_vec(&ProgramInstruction::SubmitBatchSettlement(params.clone())).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        let program_state: ProgramState = ProgramState::from_slice(&state_account.data);
        assert_eq!(
            program_state.settlement_batch_hash,
            [0u8; 32]
        );

        assert_eq!(
            hex::encode(program_state.last_settlement_batch_hash),
            hash(borsh::to_vec(&params).unwrap()),
        );
    }

    fn init_program_state_account(
        params: InitProgramStateParams,
        expected: ProgramState,
    ) {
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let expected = expected.to_vec();

        debug!("Invoking contract to init state");
        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: true
                }],
                data: borsh::to_vec(&ProgramInstruction::InitProgramState(params.clone())).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");
        debug!("submitted tx {} to arch", txid.clone());

        let _ = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");

        let account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        assert_eq!(
            expected, account.data
        )
    }

    fn init_token_state_account(
        params: InitTokenStateParams,
        token_account: Pubkey,
        expected: TokenBalances,
    ) {
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        debug!("Invoking contract to init token state");
        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: false
                    },
                    AccountMeta {
                        pubkey: token_account,
                        is_signer: false,
                        is_writable: true
                    }
                ],
                data: borsh::to_vec(&ProgramInstruction::InitTokenState(params.clone())).unwrap()
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");
        debug!("submitted tx {} to arch", txid.clone());

        let _ = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");

        let account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        assert_eq!(
            expected.to_vec(), TokenBalances::from_slice(account.data.as_slice()).to_vec()
        )
    }

    fn get_or_create_balance_index(
        address: String,
        token_account: Pubkey,
    ) -> u32 {

        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenBalances = TokenBalances::from_slice(&account_info.data);
        let len = token_balances.balances.len();
        let pos = token_balances.balances.into_iter().position(|r| r.address == address).unwrap_or_else(|| len);
        if pos == len {
            debug!("Establishing a balance index for wallet {} for token {}", address.clone(), token_balances.token_id);
            let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
            let params = InitWalletBalancesParams {
                token_balance_setups: vec![
                    TokenBalanceSetup {
                        account_index: 1,
                        wallet_addresses: vec![address.to_string()],
                    }
                ],
            };

            let (txid, _) = sign_and_send_instruction(
                Instruction {
                    program_id: SETUP.program_pubkey,
                    accounts: vec![
                        AccountMeta {
                            pubkey: submitter_pubkey,
                            is_signer: true,
                            is_writable: false
                        },
                        AccountMeta {
                            pubkey: token_account,
                            is_signer: false,
                            is_writable: true
                        }
                    ],
                    data: borsh::to_vec(&ProgramInstruction::InitWalletBalances(params.clone())).unwrap()
                },
                vec![submitter_keypair],
            ).expect("signing and sending a transaction should not fail");
            debug!("submitted tx {} to arch", txid.clone());

            let _ = get_processed_transaction(NODE1_ADDRESS, txid.clone())
                .expect("get processed transaction should not fail");
        }
        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenBalances = TokenBalances::from_slice(&account_info.data);
        token_balances.balances.into_iter().position(|r| r.address == address).unwrap() as u32
    }

    fn hash(data: Vec<u8>) -> String {
        digest(data)
    }

    fn deploy_program() -> (UntweakedKeypair, Pubkey) {
        let (program_keypair, program_pubkey) = create_new_account(PROGRAM_FILE_PATH);

        debug!("Program Account created");

        let txids = deploy_program_txs(
            program_keypair,
            "program/target/sbf-solana-solana/release/exchangeprogram.so"
        );

        debug!("Deploying Programs {:?}", txids);

        let elf = fs::read("program/target/sbf-solana-solana/release/exchangeprogram.so").expect("elf path should be available");
        assert!(read_account_info(NODE1_ADDRESS, program_pubkey.clone()).unwrap().data == elf);

        debug!("Making account executable");
        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: Pubkey::system_program(),
                accounts: vec![AccountMeta {
                    pubkey: program_pubkey.clone(),
                    is_signer: true,
                    is_writable: true
                }],
                data: vec![2]
            },
            vec![program_keypair],
        ).expect("signing and sending a transaction should not fail");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");

        assert!(read_account_info(NODE1_ADDRESS, program_pubkey.clone()).unwrap().is_executable);

        debug!("Made account executable");

        (program_keypair, program_pubkey)
    }

    fn create_new_account(file_path: &str) -> (UntweakedKeypair, Pubkey) {
        let (keypair, pubkey) = with_secret_key_file(file_path)
            .expect("getting caller info should not fail");
        debug!("Creating new account {}", file_path);
        let (txid, vout) = send_utxo(pubkey.clone());
        debug!("{}:{} {:?}", txid, vout, hex::encode(pubkey));

        let (txid, _) = sign_and_send_instruction(
            SystemInstruction::new_create_account_instruction(
                hex::decode(txid).unwrap().try_into().unwrap(),
                vout,
                pubkey.clone(),
            ),
            vec![keypair],
        ).expect("signing and sending a transaction should not fail");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");
        (keypair, pubkey)
    }

    fn assign_ownership(account_keypair: UntweakedKeypair, account_pubkey: Pubkey, program_pubkey: Pubkey) {
        let mut instruction_data = vec![3];
        instruction_data.extend(program_pubkey.serialize());

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: Pubkey::system_program(),
                accounts: vec![AccountMeta {
                    pubkey: account_pubkey.clone(),
                    is_signer: true,
                    is_writable: true
                }],
                data: instruction_data
            },
            vec![account_keypair.clone()],
        )
            .expect("Failed to sign and send Assign ownership of caller account instruction");

        let _ = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("Failed to get processed transaction");

        // 10. Verify that the program is owner of caller account
        assert_eq!(
            read_account_info(NODE1_ADDRESS, account_pubkey.clone()).unwrap().owner,
            program_pubkey,
            "Program should be owner of caller account"
        );

    }

    fn deposit_to_program(
        amount: u64,
        program_address: &Address,
    ) -> (String, u32) {

        let userpass = Auth::UserPass(
            BITCOIN_NODE_USERNAME.to_string(),
            BITCOIN_NODE_PASSWORD.to_string(),
        );
        let rpc =
            Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

        let txid = rpc
            .send_to_address(
                program_address,
                Amount::from_sat(amount),
                None,
                None,
                None,
                None,
                None,
                None,
            )
            .expect("SATs should be sent to address");

        let sent_tx = rpc
            .get_raw_transaction(&txid, None)
            .expect("should get raw transaction");
        let mut vout = 0;

        for (index, output) in sent_tx.output.iter().enumerate() {
            if output.script_pubkey == program_address.script_pubkey() {
                vout = index as u32;
            }
        }
        return (txid.to_string(), vout)
    }

    fn prepare_withdrawal(
        amount: u64,
        estimated_fee: u64,
        txid: &str,
        vout: u32
    ) -> (String, u64) {

        let userpass = Auth::UserPass(
            BITCOIN_NODE_USERNAME.to_string(),
            BITCOIN_NODE_PASSWORD.to_string(),
        );
        let rpc =
            Client::new(BITCOIN_NODE_ENDPOINT, userpass).expect("rpc shouldn not fail to be initiated");

        let txid = Txid::from_str(txid).unwrap();
        let raw_tx = rpc
            .get_raw_transaction(&txid, None)
            .expect("raw transaction should not fail");

        let prev_output = raw_tx.output[vout as usize].clone();

        if amount + estimated_fee > prev_output.value.to_sat() {
            panic!("not enough in utxo to cover amount and fee")
        }

        let change_amount = prev_output.value.to_sat() - amount - estimated_fee;

        let tx = Transaction {
            version: Version::TWO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: txid,
                    vout: vout
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![],
            lock_time: LockTime::ZERO,
        };

        (tx.raw_hex(), change_amount)
    }

    fn delete_secret_file(file_path: &str) {
        let _ = fs::remove_file(file_path);
    }
}
