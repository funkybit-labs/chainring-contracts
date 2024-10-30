/// Running Tests
///
#[cfg(test)]
mod tests {
    use common::constants::*;
    use arch_program::{pubkey::Pubkey, system_instruction::SystemInstruction, instruction::Instruction, account::AccountMeta};
    use common::helper::*;
    use std::fs;
    use std::str::FromStr;
    use bitcoin::key::UntweakedKeypair;
    use bitcoin::{Address, Amount, OutPoint, ScriptBuf, Sequence, Transaction, Txid, TxIn, TxOut, Witness};
    use bitcoin::absolute::LockTime;
    use bitcoin::transaction::Version;
    use bitcoincore_rpc::{Auth, Client, RawTx, RpcApi};

    fn cleanup_account_keys() {
        for file in vec![WALLET1_FILE_PATH, WALLET2_FILE_PATH, WALLET3_FILE_PATH, SUBMITTER_FILE_PATH, WITHDRAW_ACCOUNT_FILE_PATH, FEE_ACCOUNT_FILE_PATH] {
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
    use model::state::*;
    use model::instructions::*;

    const TOKEN_FILE_PATHS: &'static [&'static str] = &["../../data/token1.json", "../../data/token2.json"];
    pub const SUBMITTER_FILE_PATH: &str = "../../data/submitter.json";
    pub const WITHDRAW_ACCOUNT_FILE_PATH: &str = "../../data/withdraw.json";
    pub const WALLET1_FILE_PATH: &str = "../../data/wallet1.json";
    pub const WALLET2_FILE_PATH: &str = "../../data/wallet2.json";
    pub const WALLET3_FILE_PATH: &str = "../../data/wallet3.json";
    pub const FEE_ACCOUNT_FILE_PATH: &str = "../../data/fee_account.json";

    struct Setup {
        program_keypair: UntweakedKeypair,
        program_pubkey: Pubkey,
    }

    impl Setup {
        fn init() -> Self {
            env_logger::init();
            delete_secret_file(PROGRAM_FILE_PATH);
            let (program_keypair, program_pubkey) = deploy_program();
            Self {
                program_keypair,
                program_pubkey,
            }
        }
    }

    use lazy_static::lazy_static;
    use sdk::processed_transaction::*;
    use model::error::*;
    use model::serialization::Codable;

    lazy_static! {
        static ref SETUP: Setup = Setup::init();
    }

    #[test]
    fn test_deposit_and_withdrawal() {
        cleanup_account_keys();
        let accounts = onboard_state_accounts(vec!["btc"]);

        let token_account = accounts[2].clone();

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
                },
            ],
        );

        let address = get_account_address(SETUP.program_pubkey);
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
                },
            ],
        );


        let (withdraw_tx, change_amount) = prepare_withdrawal(
            5000,
            1500,
            &txid,
            vout,
        );

        // perform withdrawal
        let input = WithdrawBatchParams {
            token_withdrawals: vec![TokenWithdrawals {
                account_index: 2,
                withdrawals: vec![Withdrawal {
                    address_index: AddressIndex {
                        index: 1,
                        last4: wallet_last4(&wallet.address.to_string()),
                    },
                    amount: 5500,
                    fee_amount: 500,
                }],
            }],
            change_amount,
            tx_hex: hex::decode(withdraw_tx.clone()).unwrap(),
        };
        let expected = TokenState {
            account_type: AccountType::Token,
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
                },
            ],
        };
        assert_send_and_sign_withdrawal(
            token_account,
            input,
            expected.clone(),
            Some(3500),
            None,
        );

        let input2 = WithdrawBatchParams {
            token_withdrawals: vec![TokenWithdrawals {
                account_index: 2,
                withdrawals: vec![Withdrawal {
                    address_index: AddressIndex {
                        index: 1,
                        last4: wallet_last4(&wallet.address.to_string()),
                    },
                    amount: 100000,
                    fee_amount: 500,
                }],
            }],
            change_amount,
            tx_hex: hex::decode(withdraw_tx).unwrap(),
        };

        assert_send_and_sign_withdrawal(
            token_account,
            input2,
            expected,
            None,
            Some(
                vec![
                    Event::FailedWithdrawal {
                        account_index: 2,
                        address_index: 1,
                        requested_amount: 100000,
                        fee_amount: 500,
                        balance: 10500,
                        error_code: ERROR_INSUFFICIENT_BALANCE,
                    },
                ]
            ),
        );
    }

    #[test]
    fn test_withdrawal_partial_failure() {
        cleanup_account_keys();
        let accounts = onboard_state_accounts(vec!["btc"]);

        let token_account = accounts[2].clone();

        let wallet1 = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let wallet2 = CallerInfo::with_secret_key_file(WALLET2_FILE_PATH).unwrap();
        let wallet3 = CallerInfo::with_secret_key_file(WALLET3_FILE_PATH).unwrap();
        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();

        deposit(
            wallet1.address.to_string().clone(),
            "btc",
            token_account.clone(),
            10000,
            vec![
                Balance {
                    address: fee_account.address.to_string().clone(),
                    balance: 0,
                },
                Balance {
                    address: wallet1.address.to_string().clone(),
                    balance: 10000,
                },
            ],
        );
        deposit(
            wallet2.address.to_string().clone(),
            "btc",
            token_account.clone(),
            11000,
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
                    balance: 11000,
                },
            ],
        );
        deposit(
            wallet3.address.to_string().clone(),
            "btc",
            token_account.clone(),
            12000,
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
                    balance: 11000,
                },
                Balance {
                    address: wallet3.address.to_string().clone(),
                    balance: 12000,
                },
            ],
        );

        let address = get_account_address(SETUP.program_pubkey);
        let program_address = Address::from_str(&address)
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();

        let (txid, vout) = deposit_to_program(35000, &program_address);


        let (withdraw_tx, change_amount) = prepare_withdrawal(
            32500,
            1000,
            &txid,
            vout,
        );

        // perform withdrawal
        let input = WithdrawBatchParams {
            token_withdrawals: vec![TokenWithdrawals {
                account_index: 2,
                withdrawals: vec![
                    Withdrawal {
                        address_index: AddressIndex {
                            index: 1,
                            last4: wallet_last4(&wallet1.address.to_string()),
                        },
                        amount: 10000,
                        fee_amount: 500,
                    },
                    Withdrawal {
                        address_index: AddressIndex {
                            index: 2,
                            last4: wallet_last4(&wallet2.address.to_string()),
                        },
                        amount: 12000,
                        fee_amount: 500,
                    },
                    Withdrawal {
                        address_index: AddressIndex {
                            index: 3,
                            last4: wallet_last4(&wallet3.address.to_string()),
                        },
                        amount: 12500,
                        fee_amount: 500,
                    },
                ],
            }],
            change_amount,
            tx_hex: hex::decode(withdraw_tx.clone()).unwrap(),
        };
        let expected = TokenState {
            account_type: AccountType::Token,
            version: 0,
            program_state_account: accounts[0],
            token_id: "btc".to_string(),
            balances: vec![
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
                    balance: 11000,
                },
                Balance {
                    address: wallet3.address.to_string().clone(),
                    balance: 12000,
                },
            ],
        };
        assert_send_and_sign_withdrawal(
            token_account,
            input,
            expected.clone(),
            None,
            Some(
                vec![
                    Event::FailedWithdrawal {
                        account_index: 2,
                        address_index: 2,
                        requested_amount: 12000,
                        fee_amount: 500,
                        balance: 11000,
                        error_code: ERROR_INSUFFICIENT_BALANCE,
                    },
                    Event::FailedWithdrawal {
                        account_index: 2,
                        address_index: 3,
                        requested_amount: 12500,
                        fee_amount: 500,
                        balance: 12000,
                        error_code: ERROR_INSUFFICIENT_BALANCE,
                    },
                ]
            ),
        );
    }

    #[test]
    fn test_settlement_submission() {
        cleanup_account_keys();
        let token1 = "btc";
        let token2 = "rune1";
        let accounts = onboard_state_accounts(vec![token1, token2]);

        let token1_account = accounts[2].clone();
        let token2_account = accounts[3].clone();

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
                },
            ],
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
                },
            ],
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
                },
            ],
        };

        // prepare settlement
        assert_send_and_sign_prepare_settlement(
            accounts.clone(),
            input.clone(),
            None,
        );


        // now submit the settlement
        assert_send_and_sign_submit_settlement(
            accounts.clone(),
            input.clone(),
        );

        let token1_account_info = read_account_info(NODE1_ADDRESS, token1_account.clone()).unwrap();

        assert_eq!(
            TokenState {
                account_type: AccountType::Token,
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
                    },
                ],
            }.encode_to_vec().unwrap(),
            TokenState::decode_from_slice(token1_account_info.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );

        let token2_account_info = read_account_info(NODE1_ADDRESS, token2_account.clone()).unwrap();

        assert_eq!(
            TokenState {
                account_type: AccountType::Token,
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
                    },
                ],
            }.encode_to_vec().unwrap(),
            TokenState::decode_from_slice(token2_account_info.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );


        // start another one and make sure we can rollback
        let input2 = SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet2.address.to_string(), token1_account),
                            amount: 3500,
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet1.address.to_string(), token1_account),
                            amount: 4000,
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
                },
            ],
        };
        assert_send_and_sign_prepare_settlement(
            accounts.clone(),
            input2,
            None,
        );

        assert_send_and_sign_rollback_settlement();

        // start another one and make sure we can rollback
        let input2 = SettlementBatchParams {
            settlements: vec![
                SettlementAdjustments {
                    account_index: 1,
                    increments: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet2.address.to_string(), token1_account),
                            amount: 100500,
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet1.address.to_string(), token1_account),
                            amount: 101000,
                        }
                    ],
                    fee_amount: 500,
                },
                SettlementAdjustments {
                    account_index: 2,
                    increments: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet1.address.to_string(), token2_account),
                            amount: 100000,
                        }
                    ],
                    decrements: vec![
                        Adjustment {
                            address_index: get_or_create_balance_index(wallet2.address.to_string(), token2_account),
                            amount: 100000,
                        }
                    ],
                    fee_amount: 0,
                },
            ],
        };
        assert_send_and_sign_prepare_settlement(
            accounts.clone(),
            input2,
            Some(
                vec![
                    Event::FailedSettlement {
                        account_index: 1,
                        address_index: get_or_create_balance_index(wallet1.address.to_string(), token1_account).index,
                        requested_amount: 101000,
                        balance: 5000,
                        error_code: ERROR_INSUFFICIENT_BALANCE,
                    },
                    Event::FailedSettlement {
                        account_index: 2,
                        address_index: get_or_create_balance_index(wallet2.address.to_string(), token2_account).index,
                        requested_amount: 100000,
                        balance: 7000,
                        error_code: ERROR_INSUFFICIENT_BALANCE,
                    },
                ]
            ),
        );
    }

    #[test]
    fn test_deposits_to_multiple_wallets() {
        cleanup_account_keys();
        let token1 = "btc";
        let accounts = onboard_state_accounts(vec![token1]);

        let token1_account = accounts[2].clone();

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
                },
            ],
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
                },
            ],
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
                },
            ],
        );
    }

    #[test]
    fn test_setup_many_wallets() {
        cleanup_account_keys();
        let token1 = "btc";
        let accounts = onboard_state_accounts(vec![token1]);

        let withdraw_pubkey = accounts[1].clone();
        let token_account = accounts[2].clone();
        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenState = TokenState::decode_from_slice(&account_info.data).unwrap();
        assert_eq!(1, token_balances.balances.len());
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        // create 1000 wallets
        let wallets = (0..1000)
            .map(|_| CallerInfo::generate_new().unwrap().address.to_string())
            .collect::<Vec<String>>();

        // send in chunks not to exceed max instruction size
        let txids: Vec<String> = wallets
            .chunks(25)
            .enumerate()
            .map(|(i, chunk)| {
                let params = InitWalletBalancesParams {
                    token_state_setups: vec![
                        TokenStateSetup {
                            account_index: 1,
                            wallet_addresses: chunk.to_vec(),
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
                                is_writable: false,
                            },
                            AccountMeta {
                                pubkey: token_account,
                                is_signer: false,
                                is_writable: true,
                            },
                        ],
                        data: ProgramInstruction::InitWalletBalances(params.clone()).encode_to_vec().unwrap(),
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
        let token_balances: TokenState = TokenState::decode_from_slice(&account_info.data).unwrap();
        assert_eq!(wallets.len() + 1, token_balances.balances.len());

        for (i, wallet) in wallets.iter().enumerate() {
            assert_eq!(
                i + 1,
                token_balances.balances.clone().into_iter().position(|r| r.address == *wallet).unwrap()
            )
        }

        let mut adjustments: Vec<Adjustment> = vec![];
        for index in 0..100 {
            adjustments.push(
                Adjustment {
                    address_index: AddressIndex {
                        index: index + 1,
                        last4: wallet_last4(&wallets[index as usize]),
                    },
                    amount: 10000,
                }
            )
        }

        // deposit to first 100
        sign_and_send_instruction_success(
            vec![
                AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: false,
                },
                AccountMeta {
                    pubkey: token_account,
                    is_signer: false,
                    is_writable: true,
                },
            ],
            ProgramInstruction::BatchDeposit(
                DepositBatchParams {
                    token_deposits: vec![
                        TokenDeposits {
                            account_index: 1,
                            deposits: adjustments
                        }
                    ],
                }
            ).encode_to_vec().unwrap(),
            vec![submitter_keypair],
        );

        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenState = TokenState::decode_from_slice(&account_info.data).unwrap();
        assert_eq!(wallets.len() + 1, token_balances.balances.len());

        for (i, _) in wallets.iter().enumerate() {
            if i < 100 {
                assert_eq!(token_balances.balances[i + 1].balance, 10000)
            } else {
                assert_eq!(token_balances.balances[i + 1].balance, 0)
            }
        }

        let address = get_account_address(SETUP.program_pubkey);
        let program_address = Address::from_str(&address)
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();

        let num_withdrawals_per_batch = 7;
        let num_withdrawal_batches = 5;
        let txs = (0..num_withdrawal_batches * num_withdrawals_per_batch).enumerate().map(
            |_| deposit_to_program(8000, &program_address)
        ).collect::<Vec<(String, u32)>>();
        let mut utxo_index: usize = 0;


        for index in 0..num_withdrawal_batches {
            mine();
            let tx = Transaction {
                version: Version::TWO,
                input: (0..num_withdrawals_per_batch).map(|i|
                    TxIn {
                        previous_output: OutPoint {
                            txid: Txid::from_str(&txs[utxo_index + i].0).unwrap(),
                            vout: txs[utxo_index + i].1,
                        },
                        script_sig: ScriptBuf::new(),
                        sequence: Sequence::MAX,
                        witness: Witness::new(),
                    }
                ).collect::<Vec<TxIn>>(),
                output: vec![],
                lock_time: LockTime::ZERO,
            };
            utxo_index += num_withdrawals_per_batch;
            let mut withdrawals: Vec<Withdrawal> = vec![];
            for i in 0..num_withdrawals_per_batch {
                withdrawals.push(
                    Withdrawal {
                        address_index: AddressIndex {
                            index: (num_withdrawals_per_batch * index + i + 1) as u32,
                            last4: wallet_last4(&wallets[num_withdrawals_per_batch * index + i]),
                        },
                        amount: 6000,
                        fee_amount: 0,
                    }
                )
            }
            let withdraw_batch_params = WithdrawBatchParams {
                tx_hex: hex::decode(tx.raw_hex()).unwrap(),
                change_amount: (1000 * num_withdrawals_per_batch) as u64,
                token_withdrawals: vec![
                    TokenWithdrawals {
                        account_index: 2,
                        withdrawals,
                    }
                ],
            };
            sign_and_send_instruction_success(
                vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: withdraw_pubkey,
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: token_account,
                        is_signer: false,
                        is_writable: true,
                    },
                ],
                ProgramInstruction::PrepareBatchWithdraw(
                    withdraw_batch_params.clone()
                ).encode_to_vec().unwrap(),
                vec![submitter_keypair],
            );
            let (withdraw_keypair, _) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
            let processed_tx = sign_and_send_instruction_success(
                vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: false,
                    },
                    AccountMeta {
                        pubkey: withdraw_pubkey,
                        is_signer: true,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: token_account,
                        is_signer: false,
                        is_writable: false,
                    },
                ],
                ProgramInstruction::SubmitBatchWithdraw(
                    withdraw_batch_params
                ).encode_to_vec().unwrap(),
                vec![submitter_keypair, withdraw_keypair],
            );
            assert_ne!(processed_tx.bitcoin_txid, None);
            debug!("processed tx = {:?}", processed_tx.bitcoin_txid)
        }

        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenState = TokenState::decode_from_slice(&account_info.data).unwrap();
        assert_eq!(wallets.len() + 1, token_balances.balances.len());

        for (i, _) in wallets.iter().enumerate() {
            if i < num_withdrawal_batches * num_withdrawals_per_batch {
                assert_eq!(token_balances.balances[i + 1].balance, 4000)
            } else if i < 100 {
                assert_eq!(token_balances.balances[i + 1].balance, 10000)
            } else {
                assert_eq!(token_balances.balances[i + 1].balance, 0)
            }
        }

    }

    #[test]
    fn test_withdrawal_rollback() {
        cleanup_account_keys();
        let accounts = onboard_state_accounts(vec!["btc"]);

        let token_account = accounts[2].clone();

        let wallet = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();

        let address = get_account_address(SETUP.program_pubkey);
        let program_address = Address::from_str(&address)
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();

        let (txid, vout) = deposit_to_program(10000, &program_address);

        let balances_after_deposit = vec![
            Balance {
                address: fee_account.address.to_string().clone(),
                balance: 0,
            },
            Balance {
                address: wallet.address.to_string().clone(),
                balance: 10000,
            },
        ];

        deposit(
            wallet.address.to_string().clone(),
            "btc",
            token_account.clone(),
            10000,
            balances_after_deposit.clone(),
        );

        let (withdraw_tx, change_amount) = prepare_withdrawal(
            5000,
            1500,
            &txid,
            vout,
        );

        let token_withdrawals = vec![TokenWithdrawals {
            account_index: 2,
            withdrawals: vec![Withdrawal {
                address_index: AddressIndex {
                    index: 1,
                    last4: wallet_last4(&wallet.address.to_string()),
                },
                amount: 5500,
                fee_amount: 500,
            }],
        }];

        // perform withdrawal
        assert_send_and_sign_withdrawal(
            token_account,
            WithdrawBatchParams {
                token_withdrawals: token_withdrawals.clone(),
                change_amount,
                tx_hex: hex::decode(withdraw_tx).unwrap(),
            },
            TokenState {
                account_type: AccountType::Token,
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
                        balance: 4500,
                    },
                ],
            },
            Some(3500),
            None,
        );

        assert_send_and_sign_withdrawal_rollback(
            token_account,
            RollbackWithdrawBatchParams {
                token_withdrawals,
            },
            TokenState {
                account_type: AccountType::Token,
                version: 0,
                program_state_account: accounts[0],
                token_id: "btc".to_string(),
                balances: balances_after_deposit,
            },
        );
    }

    #[test]
    fn test_errors() {
        cleanup_account_keys();
        let accounts = onboard_state_accounts(vec!["btc"]);

        let withdraw_account = accounts[1].clone();
        let token_account = accounts[2].clone();
        let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        // cannot re-init program account
        test_error_condition(
            vec![
                AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: withdraw_account,
                    is_signer: false,
                    is_writable: true,
                },
            ],
            ProgramInstruction::InitProgramState(
                InitProgramStateParams {
                    fee_account: fee_account.address.to_string(),
                    program_change_address: fee_account.address.clone().to_string(),
                    network_type: NetworkType::Regtest,
                }
            ),
            ERROR_ALREADY_INITIALIZED,
        );

        // cannot reinit token account
        let program_and_token_acct = vec![
            AccountMeta {
                pubkey: submitter_pubkey,
                is_signer: true,
                is_writable: false,
            },
            AccountMeta {
                pubkey: token_account,
                is_signer: false,
                is_writable: true,
            },
        ];
        test_error_condition(
            program_and_token_acct.clone(),
            ProgramInstruction::InitTokenState(
                InitTokenStateParams {
                    token_id: "btc2".to_string(),
                }
            ),
            ERROR_ALREADY_INITIALIZED,
        );

        // invalid account index
        test_error_condition(
            program_and_token_acct.clone(),
            ProgramInstruction::InitWalletBalances(
                InitWalletBalancesParams {
                    token_state_setups: vec![
                        TokenStateSetup {
                            account_index: 3,
                            wallet_addresses: vec![],
                        }
                    ],
                }
            ),
            ERROR_INVALID_ACCOUNT_INDEX,
        );

        // invalid wallet address
        test_error_condition(
            program_and_token_acct.clone(),
            ProgramInstruction::InitWalletBalances(
                InitWalletBalancesParams {
                    token_state_setups: vec![
                        TokenStateSetup {
                            account_index: 1,
                            wallet_addresses: vec!["bc1rt12345456667".to_string()],
                        }
                    ],
                }
            ),
            ERROR_INVALID_ADDRESS,
        );

        // invalid wallet address network
        test_error_condition(
            program_and_token_acct.clone(),
            ProgramInstruction::InitWalletBalances(
                InitWalletBalancesParams {
                    token_state_setups: vec![
                        TokenStateSetup {
                            account_index: 1,
                            wallet_addresses: vec!["bc1qar0srrr7xfkvy5l643lydnw9re59gtzzwf5mdq".to_string()],
                        }
                    ],
                }
            ),
            ERROR_INVALID_ADDRESS_NETWORK,
        );

        // invalid address index
        test_error_condition(
            program_and_token_acct.clone(),
            ProgramInstruction::BatchDeposit(
                DepositBatchParams {
                    token_deposits: vec![
                        TokenDeposits {
                            account_index: 1,
                            deposits: vec![
                                Adjustment {
                                    address_index: AddressIndex { index: 1000, last4: [0; 4] },
                                    amount: 0,
                                }
                            ],
                        }
                    ],

                }
            ),
            ERROR_INVALID_ADDRESS_INDEX,
        );

        // invalid address last 4
        test_error_condition(
            program_and_token_acct.clone(),
            ProgramInstruction::BatchDeposit(
                DepositBatchParams {
                    token_deposits: vec![
                        TokenDeposits {
                            account_index: 1,
                            deposits: vec![
                                Adjustment {
                                    address_index: AddressIndex { index: 0, last4: [0; 4] },
                                    amount: 0,
                                }
                            ],
                        }
                    ],

                }
            ),
            ERROR_WALLET_LAST4_MISMATCH,
        );

        // signer account is not the program account
        test_error_condition(
            vec![
                AccountMeta {
                    pubkey: withdraw_account,
                    is_signer: true,
                    is_writable: false,
                },
                AccountMeta {
                    pubkey: token_account,
                    is_signer: false,
                    is_writable: true,
                },
            ],
            ProgramInstruction::InitWalletBalances(
                InitWalletBalancesParams {
                    token_state_setups: vec![
                        TokenStateSetup {
                            account_index: 1,
                            wallet_addresses: vec![],
                        }
                    ],
                }
            ),
            ERROR_INVALID_ACCOUNT_TYPE,
        );

        // program/submitter does not sign the request
        test_error_condition(
            vec![
                AccountMeta {
                    pubkey: withdraw_account,
                    is_signer: false,
                    is_writable: false,
                },
                AccountMeta {
                    pubkey: token_account,
                    is_signer: false,
                    is_writable: true,
                },
            ],
            ProgramInstruction::InitWalletBalances(
                InitWalletBalancesParams {
                    token_state_setups: vec![
                        TokenStateSetup {
                            account_index: 1,
                            wallet_addresses: vec![],
                        }
                    ],
                }
            ),
            ERROR_INVALID_ACCOUNT_FLAGS,
        );

        let mut withdraw_batch_params = WithdrawBatchParams {
            tx_hex: vec![],
            change_amount: 0,
            token_withdrawals: vec![
                TokenWithdrawals {
                    account_index: 1,
                    withdrawals: vec![
                        Withdrawal {
                            address_index: AddressIndex { index: 0, last4: wallet_last4(&fee_account.address.to_string()) },
                            amount: 100000000,
                            fee_amount: 0,
                        }
                    ],
                }
            ],
        };

        let withdraw_accounts = vec![
            AccountMeta {
                pubkey: submitter_pubkey,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: withdraw_account,
                is_signer: false,
                is_writable: true,
            },
            AccountMeta {
                pubkey: token_account,
                is_signer: false,
                is_writable: true,
            },
        ];

        // withdraw but send in no input txs
        test_error_condition(
            withdraw_accounts.clone(),
            ProgramInstruction::PrepareBatchWithdraw(
                withdraw_batch_params.clone()
            ),
            ERROR_INVALID_INPUT_TX,
        );

        // withdraw but add an output to the input tx - no outputs allowed
        let tx = Transaction {
            version: Version::TWO,
            input: vec![TxIn {
                previous_output: OutPoint {
                    txid: Txid::from_str("0000000000000000000000000000000000000000000000000000000000000000").unwrap(),
                    vout: 0,
                },
                script_sig: ScriptBuf::new(),
                sequence: Sequence::MAX,
                witness: Witness::new(),
            }],
            output: vec![TxOut {
                value: Amount::from_sat(1000),
                script_pubkey: Default::default(),
            }],
            lock_time: LockTime::ZERO,
        };
        withdraw_batch_params.tx_hex = hex::decode(tx.raw_hex()).unwrap();
        test_error_condition(
            withdraw_accounts.clone(),
            ProgramInstruction::PrepareBatchWithdraw(
                withdraw_batch_params.clone()
            ),
            ERROR_NO_OUTPUTS_ALLOWED,
        );

        // settlement errors

        let settlement_accounts = vec![
            AccountMeta {
                pubkey: submitter_pubkey,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: token_account,
                is_signer: false,
                is_writable: false,
            },
        ];

        // netting error
        test_error_condition(
            settlement_accounts.clone(),
            ProgramInstruction::PrepareBatchSettlement(
                SettlementBatchParams {
                    settlements: vec![
                        SettlementAdjustments {
                            account_index: 1,
                            increments: vec![],
                            decrements: vec![],
                            fee_amount: 2,
                        }
                    ],
                }
            ),
            ERROR_NETTING,
        );

        // cannot submit if not prepared
        test_error_condition(
            settlement_accounts.clone(),
            ProgramInstruction::SubmitBatchSettlement(
                SettlementBatchParams {
                    settlements: vec![
                        SettlementAdjustments {
                            account_index: 1,
                            increments: vec![],
                            decrements: vec![],
                            fee_amount: 0,
                        }
                    ],
                }
            ),
            ERROR_NO_SETTLEMENT_IN_PROGRESS,
        );

        // prepare a settlement
        sign_and_send_instruction_success(
            settlement_accounts.clone(),
            ProgramInstruction::PrepareBatchSettlement(
                SettlementBatchParams {
                    settlements: vec![
                        SettlementAdjustments {
                            account_index: 1,
                            increments: vec![],
                            decrements: vec![],
                            fee_amount: 0,
                        }
                    ],
                }
            ).encode_to_vec().unwrap(),
            vec![submitter_keypair],
        );

        // try to prepare again - should fail
        test_error_condition(
            settlement_accounts.clone(),
            ProgramInstruction::PrepareBatchSettlement(
                SettlementBatchParams {
                    settlements: vec![
                        SettlementAdjustments {
                            account_index: 1,
                            increments: vec![],
                            decrements: vec![],
                            fee_amount: 3,
                        }
                    ],
                }
            ),
            ERROR_SETTLEMENT_IN_PROGRESS,
        );

        // submit with a different hash
        test_error_condition(
            settlement_accounts.clone(),
            ProgramInstruction::SubmitBatchSettlement(
                SettlementBatchParams {
                    settlements: vec![
                        SettlementAdjustments {
                            account_index: 1,
                            increments: vec![],
                            decrements: vec![],
                            fee_amount: 4,
                        }
                    ],
                }
            ),
            ERROR_SETTLEMENT_BATCH_MISMATCH,
        );

        // try to withdraw - should fail if settlement in progress
        test_error_condition(
            withdraw_accounts.clone(),
            ProgramInstruction::PrepareBatchWithdraw(
                WithdrawBatchParams {
                    tx_hex: vec![],
                    change_amount: 0,
                    token_withdrawals: vec![],
                }
            ),
            ERROR_SETTLEMENT_IN_PROGRESS,
        );
    }

    fn test_error_condition(
        accounts: Vec<AccountMeta>,
        instruction: ProgramInstruction,
        expected_custom_error_code: u32,
    ) {
        let (submitter_keypair, _) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts,
                data: instruction.encode_to_vec().unwrap(),
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");
        validate_error(processed_tx.clone(), expected_custom_error_code);
    }

    fn sign_and_send_token_instruction_success(
        token_account: Option<Pubkey>,
        instruction: ProgramInstruction,
        withdraw_account: Option<Pubkey>,
        program_state_is_writable: bool,
    ) -> ProcessedTransaction {
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let mut accounts = vec![
            AccountMeta {
                pubkey: submitter_pubkey,
                is_signer: true,
                is_writable: program_state_is_writable,
            }
        ];
        if let Some(withdraw_pubkey) = withdraw_account {
            accounts.push(
                AccountMeta {
                    pubkey: withdraw_pubkey,
                    is_signer: false,
                    is_writable: true,
                }
            );
        }
        if let Some(token_pubkey) = token_account {
            accounts.push(
                AccountMeta {
                    pubkey: token_pubkey,
                    is_signer: false,
                    is_writable: true,
                },
            );
        }
        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts,
                data: instruction.encode_to_vec().unwrap(),
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");
        assert_eq!(processed_tx.status, Status::Processed);
        processed_tx
    }

    fn sign_and_send_instruction_success(
        accounts: Vec<AccountMeta>,
        instruction_bytes: Vec<u8>,
        signers: Vec<UntweakedKeypair>,
    ) -> ProcessedTransaction {
        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts,
                data: instruction_bytes,
            },
            signers,
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");
        assert_eq!(processed_tx.status, Status::Processed);
        processed_tx
    }


    // support functions
    fn deposit(
        address: String,
        token: &str,
        token_account: Pubkey,
        amount: u64,
        expected_balances: Vec<Balance>,
    ) {
        let (_, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

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
        let expected = TokenState {
            account_type: AccountType::Token,
            version: 0,
            program_state_account: submitter_pubkey,
            token_id: token.to_string(),
            balances: expected_balances,
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

        let (withdraw_account_keypair, withdraw_account_pubkey) = create_new_account(WITHDRAW_ACCOUNT_FILE_PATH);
        debug!("Created withdraw account");

        assign_ownership(withdraw_account_keypair, withdraw_account_pubkey, SETUP.program_pubkey.clone());
        debug!("Assigned ownership for withdraw account");

        let program_change_address = get_account_address(SETUP.program_pubkey);

        init_program_state_account(
            InitProgramStateParams {
                fee_account: fee_account.address.to_string(),
                program_change_address: program_change_address.clone(),
                network_type: NetworkType::Regtest,
            },
            ProgramState {
                account_type: AccountType::Program,
                version: 0,
                withdraw_account: withdraw_account_pubkey,
                fee_account_address: fee_account.address.to_string(),
                program_change_address,
                network_type: NetworkType::Regtest,
                settlement_batch_hash: EMPTY_HASH,
                last_settlement_batch_hash: EMPTY_HASH,
                events: vec![],
            },
        );
        debug!("Initialized program state");
        accounts.push(submitter_pubkey);
        accounts.push(withdraw_account_pubkey);

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
                TokenState {
                    account_type: AccountType::Token,
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
        expected: TokenState,
    ) {
        debug!("Performing Deposit");
        sign_and_send_token_instruction_success(
            Some(token_account),
            ProgramInstruction::BatchDeposit(params.clone()),
            None,
            false,
        );

        let token_account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        assert_eq!(
            expected.encode_to_vec().unwrap(), TokenState::decode_from_slice(token_account.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );
    }

    fn assert_send_and_sign_withdrawal(
        token_account: Pubkey,
        params: WithdrawBatchParams,
        expected: TokenState,
        expected_change_amount: Option<u64>,
        expected_events: Option<Vec<Event>>,
    ) {
        debug!("Performing Withdrawal");
        let expected = expected.encode_to_vec().unwrap();
        let wallet = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
        let (withdraw_keypair, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let program_change_address = Address::from_str(&get_account_address(SETUP.program_pubkey))
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();
        let withdraw_account_address = Address::from_str(&get_account_address(withdraw_pubkey))
            .unwrap()
            .require_network(bitcoin::Network::Regtest)
            .unwrap();


        let processed_tx = sign_and_send_instruction_success(
            vec![
                AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: withdraw_pubkey,
                    is_signer: false,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: token_account,
                    is_signer: false,
                    is_writable: true,
                },
            ],
            ProgramInstruction::PrepareBatchWithdraw(params.clone()).encode_to_vec().unwrap(),
            vec![submitter_keypair],
        );

        assert_eq!(processed_tx.bitcoin_txid, None);
        let token_state_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        assert_eq!(
            expected, TokenState::decode_from_slice(token_state_info.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );

        let withdraw_account_info = read_account_info(NODE1_ADDRESS, withdraw_pubkey).unwrap();
        let withdraw_state = WithdrawState::decode_from_slice(withdraw_account_info.data.as_slice()).unwrap();
        assert_eq!(AccountType::Withdraw, withdraw_state.account_type);
        let withdraw_utxo_before = withdraw_account_info.utxo;

        if let Some(events) = expected_events {
            let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
            let program_state: ProgramState = ProgramState::decode_from_slice(&state_account.data).unwrap();
            assert_eq!(
                program_state.events,
                events
            );
            assert_eq!(
                withdraw_state.batch_hash,
                EMPTY_HASH
            );
            return;
        }

        assert_eq!(
            hex::encode(withdraw_state.batch_hash),
            hash(&params.encode_to_vec().unwrap()),
        );

        let processed_tx = sign_and_send_instruction_success(
            vec![
                AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: false,
                },
                AccountMeta {
                    pubkey: withdraw_pubkey,
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: token_account,
                    is_signer: false,
                    is_writable: false,
                },
            ],
            ProgramInstruction::SubmitBatchWithdraw(params.clone()).encode_to_vec().unwrap(),
            vec![submitter_keypair, withdraw_keypair],
        );

        let token_state_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        assert_eq!(
            expected, TokenState::decode_from_slice(token_state_info.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );
        let withdraw_account_info = read_account_info(NODE1_ADDRESS, withdraw_pubkey).unwrap();
        let withdraw_state = WithdrawState::decode_from_slice(withdraw_account_info.data.as_slice()).unwrap();
        assert_eq!(
            withdraw_state.batch_hash,
            EMPTY_HASH
        );
        assert_ne!(withdraw_account_info.utxo, withdraw_utxo_before);


        if let Some(expected_change_amount) = expected_change_amount {
            let bitcoin_txid = match processed_tx.bitcoin_txid {
                Some(x) => Txid::from_str(&x).unwrap(),
                None => Txid::from_str("").unwrap(),
            };
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
            let mut withdraw_account_vout: u32 = 10000;
            let mut vout: u32 = 0;

            for output in sent_tx.output.iter() {
                if output.script_pubkey == wallet.address.script_pubkey() {
                    wallet_amount = output.value.to_sat();
                }
                if output.script_pubkey == program_change_address.script_pubkey() {
                    change_amount = output.value.to_sat();
                }
                if output.script_pubkey == withdraw_account_address.script_pubkey() {
                    withdraw_account_vout = vout
                }
                vout = vout + 1;
            }
            assert_eq!(
                withdraw_account_info.utxo,
                format!("{}:{}", &bitcoin_txid, withdraw_account_vout)
            );
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
    }

    fn assert_send_and_sign_withdrawal_rollback(
        token_account: Pubkey,
        params: RollbackWithdrawBatchParams,
        expected: TokenState,
    ) {
        debug!("Performing Withdrawal Rollback");
        let expected = expected.encode_to_vec().unwrap();
        let (_, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();

        sign_and_send_token_instruction_success(
            Some(token_account),
            ProgramInstruction::RollbackBatchWithdraw(params.clone()),
            Some(withdraw_pubkey),
            false,
        );

        let token_account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        assert_eq!(
            expected, TokenState::decode_from_slice(token_account.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );
    }

    fn assert_send_and_sign_prepare_settlement(
        accounts: Vec<Pubkey>,
        params: SettlementBatchParams,
        expected_events: Option<Vec<Event>>,
    ) {
        debug!("Performing prepare Settlement Batch");
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: accounts[2],
                        is_signer: false,
                        is_writable: false,
                    },
                    AccountMeta {
                        pubkey: accounts[3],
                        is_signer: false,
                        is_writable: false,
                    },
                ],
                data: ProgramInstruction::PrepareBatchSettlement(params.clone()).encode_to_vec().unwrap(),
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");
        assert_eq!(processed_tx.status, Status::Processed);

        let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        let program_state: ProgramState = ProgramState::decode_from_slice(&state_account.data).unwrap();
        if let Some(events) = expected_events {
            assert_eq!(
                program_state.settlement_batch_hash,
                EMPTY_HASH,
            );
            assert_eq!(
                program_state.events,
                events
            )
        } else {
            assert_eq!(
                hex::encode(program_state.settlement_batch_hash),
                hash(&params.encode_to_vec().unwrap()),
            );
        }
    }

    fn assert_send_and_sign_rollback_settlement() {
        debug!("Performing rollback Settlement Batch");
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        sign_and_send_instruction_success(
            vec![
                AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: true,
                },
            ],
            ProgramInstruction::RollbackBatchSettlement().encode_to_vec().unwrap(),
            vec![submitter_keypair],
        );

        let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        let program_state: ProgramState = ProgramState::decode_from_slice(&state_account.data).unwrap();
        assert_eq!(
            EMPTY_HASH,
            program_state.settlement_batch_hash
        );
    }


    fn assert_send_and_sign_submit_settlement(
        accounts: Vec<Pubkey>,
        params: SettlementBatchParams,
    ) {
        debug!("Performing submit Settlement Batch");
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let (txid, _) = sign_and_send_instruction(
            Instruction {
                program_id: SETUP.program_pubkey,
                accounts: vec![
                    AccountMeta {
                        pubkey: submitter_pubkey,
                        is_signer: true,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: accounts[2],
                        is_signer: false,
                        is_writable: true,
                    },
                    AccountMeta {
                        pubkey: accounts[3],
                        is_signer: false,
                        is_writable: true,
                    },
                ],
                data: ProgramInstruction::SubmitBatchSettlement(params.clone()).encode_to_vec().unwrap(),
            },
            vec![submitter_keypair],
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");
        assert_eq!(processed_tx.status, Status::Processed);

        let state_account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        let program_state: ProgramState = ProgramState::decode_from_slice(&state_account.data).unwrap();
        assert_eq!(
            program_state.settlement_batch_hash,
            EMPTY_HASH
        );

        assert_eq!(
            hash(&params.encode_to_vec().unwrap()),
            hex::encode(program_state.last_settlement_batch_hash),
        );
    }

    fn init_program_state_account(
        params: InitProgramStateParams,
        expected: ProgramState,
    ) {
        let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let (_, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
        let expected = expected.encode_to_vec().unwrap();

        debug!("Invoking contract to init program state");
        let processed_tx = sign_and_send_instruction_success(
            vec![
                AccountMeta {
                    pubkey: submitter_pubkey,
                    is_signer: true,
                    is_writable: true,
                },
                AccountMeta {
                    pubkey: withdraw_pubkey,
                    is_signer: false,
                    is_writable: true,
                },
            ],
            ProgramInstruction::InitProgramState(params.clone()).encode_to_vec().unwrap(),
            vec![submitter_keypair],
        );
        debug!("processed_tx = {:?}", processed_tx);

        let account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
        assert_eq!(
            expected, ProgramState::decode_from_slice(account.data.as_slice()).unwrap().encode_to_vec().unwrap()
        );

        let account = read_account_info(NODE1_ADDRESS, withdraw_pubkey.clone()).unwrap();
        let withdraw_state = WithdrawState::decode_from_slice(account.data.as_slice()).unwrap();
        assert_eq!(submitter_pubkey, withdraw_state.program_state_account);
        assert_eq!(EMPTY_HASH, withdraw_state.batch_hash);
    }

    fn init_token_state_account(
        params: InitTokenStateParams,
        token_account: Pubkey,
        expected: TokenState,
    ) {
        debug!("Invoking contract to init token state");
        sign_and_send_token_instruction_success(
            Some(token_account),
            ProgramInstruction::InitTokenState(params.clone()),
            None,
            false,
        );

        let account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        assert_eq!(
            expected.encode_to_vec().unwrap(), TokenState::decode_from_slice(account.data.as_slice()).unwrap().encode_to_vec().unwrap()
        )
    }

    fn get_or_create_balance_index(
        address: String,
        token_account: Pubkey,
    ) -> AddressIndex {
        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_state: TokenState = TokenState::decode_from_slice(&account_info.data).unwrap();
        let len = token_state.balances.len();
        let pos = token_state.balances.into_iter().position(|r| r.address == address).unwrap_or_else(|| len);
        if pos == len {
            debug!("Establishing a balance index for wallet {} for token {}", address.clone(), token_state.token_id);
            sign_and_send_token_instruction_success(
                Some(token_account),
                ProgramInstruction::InitWalletBalances(
                    InitWalletBalancesParams {
                        token_state_setups: vec![
                            TokenStateSetup {
                                account_index: 1,
                                wallet_addresses: vec![address.to_string()],
                            }
                        ],
                    }
                ),
                None,
                false,
            );
        }
        let account_info = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
        let token_balances: TokenState = TokenState::decode_from_slice(&account_info.data).unwrap();
        AddressIndex {
            index: token_balances.balances.into_iter().position(|r| r.address == address).unwrap() as u32,
            last4: wallet_last4(&address),
        }
    }

    fn hash(data: &[u8]) -> String {
        digest(data)
    }

    fn deploy_program() -> (UntweakedKeypair, Pubkey) {
        let (program_keypair, program_pubkey) = create_new_account(PROGRAM_FILE_PATH);

        debug!("Program Account created");

        let txids = deploy_program_txs(
            program_keypair,
            "program/target/sbf-solana-solana/release/exchangeprogram.so",
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
                    is_writable: true,
                }],
                data: vec![2],
            },
            vec![program_keypair],
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");
        assert_eq!(processed_tx.status, Status::Processed);

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

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("get processed transaction should not fail");
        assert_eq!(processed_tx.status, Status::Processed);
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
                    is_writable: true,
                }],
                data: instruction_data,
            },
            vec![account_keypair.clone()],
        )
            .expect("Failed to sign and send Assign ownership of caller account instruction");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
            .expect("Failed to get processed transaction");
        assert_eq!(processed_tx.status, Status::Processed);

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
        return (txid.to_string(), vout);
    }

    fn prepare_withdrawal(
        amount: u64,
        estimated_fee: u64,
        txid: &str,
        vout: u32,
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
                    vout: vout,
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

    fn validate_error(processed_tx: ProcessedTransaction, expected_status_code: u32) {
        let expected_custom_msg = format!("Custom program error: 0x{:x}", expected_status_code);
        match processed_tx.status {
            Status::Failed(value) => assert!(value.contains(&expected_custom_msg), "unexpected error"),
            Status::Processed => assert!(false, "status is Processed"),
            Status::Processing => assert!(false, "status is Processing")
        }
    }
}
