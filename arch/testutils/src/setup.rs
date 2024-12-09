use std::fs;
use bitcoin::{Address, Amount, Txid};
use bitcoin::key::UntweakedKeypair;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use common::constants::{BITCOIN_NODE_ENDPOINT, BITCOIN_NODE_PASSWORD, BITCOIN_NODE_USERNAME, NODE1_ADDRESS, PROGRAM_FILE_PATH};
use arch_program::{pubkey::Pubkey, instruction::Instruction, account::AccountMeta, system_instruction::SystemInstruction};
use common::helper::{deploy_program_txs, get_account_address, get_processed_transaction, read_account_info, send_utxo, sign_and_send_instruction, with_secret_key_file};
use common::models::CallerInfo;
use common::processed_transaction::{ProcessedTransaction, Status};
use crate::bitcoin::mine;
use crate::constants::{FEE_ACCOUNT_FILE_PATH, RUNE_RECEIVER_ACCOUNT_FILE_PATH, SUBMITTER_FILE_PATH, TOKEN_FILE_PATHS, WALLET1_FILE_PATH, WITHDRAW_ACCOUNT_FILE_PATH};
use crate::utils::hash;
use log::debug;
use model::state::*;
use model::instructions::*;
use model::serialization::Codable;
use std::str::FromStr;

pub fn sign_and_send_instruction_success(
    accounts: Vec<AccountMeta>,
    instruction_bytes: Vec<u8>,
    signers: Vec<UntweakedKeypair>,
) -> ProcessedTransaction {
    let (_, program_pubkey) = with_secret_key_file(PROGRAM_FILE_PATH).unwrap();
    let (txid, _) = sign_and_send_instruction(
        Instruction {
            program_id: program_pubkey,
            accounts,
            data: instruction_bytes,
        },
        signers,
    ).expect("signing and sending a transaction should not fail");

    let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
        .expect("get processed transaction should not fail");
    debug!("sign_and_send: {:?}", processed_tx);
    assert_eq!(processed_tx.status, Status::Processed);
    processed_tx
}

pub fn sign_and_send_token_instruction_success(
    token_account: Option<Pubkey>,
    instruction: ProgramInstruction,
    withdraw_account: Option<Pubkey>,
    program_state_is_writable: bool,
) -> ProcessedTransaction {
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    let (_, program_pubkey) = with_secret_key_file(PROGRAM_FILE_PATH).unwrap();
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
            program_id: program_pubkey,
            accounts,
            data: instruction.encode_to_vec().unwrap(),
        },
        vec![submitter_keypair],
    ).expect("signing and sending a transaction should not fail");

    let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid.clone())
        .expect("get processed transaction should not fail");
    debug!("sign_and_send: {:?}", processed_tx);
    assert_eq!(processed_tx.status, Status::Processed);
    processed_tx
}


pub fn deploy_program() -> (UntweakedKeypair, Pubkey) {
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
    debug!("make account executable: {:?}", processed_tx);
    assert_eq!(processed_tx.status, Status::Processed);

    assert!(read_account_info(NODE1_ADDRESS, program_pubkey.clone()).unwrap().is_executable);

    debug!("Made account executable");

    (program_keypair, program_pubkey)
}

pub fn onboard_state_accounts(tokens: Vec<&str>) -> Vec<Pubkey> {
    debug!("Performing onboard program state");

    let fee_account = CallerInfo::with_secret_key_file(FEE_ACCOUNT_FILE_PATH).unwrap();
    let (_, program_pubkey) = with_secret_key_file(PROGRAM_FILE_PATH).unwrap();

    let mut accounts: Vec<Pubkey> = vec![];
    let (submitter_keypair, submitter_pubkey) = create_new_account(SUBMITTER_FILE_PATH);
    debug!("Created program state account");

    assign_ownership(submitter_keypair, submitter_pubkey, program_pubkey);
    debug!("Assigned ownership for program state account");

    let (withdraw_account_keypair, withdraw_account_pubkey) = create_new_account(WITHDRAW_ACCOUNT_FILE_PATH);
    debug!("Created withdraw account");

    assign_ownership(withdraw_account_keypair, withdraw_account_pubkey, program_pubkey);
    debug!("Assigned ownership for withdraw account");

    let program_change_address = get_account_address(program_pubkey);

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
        assign_ownership(token_keypair, token_pubkey, program_pubkey.clone());
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
                balances: if !TokenState::is_rune_id(token) {
                    vec![Balance {
                        address: fee_account.address.to_string(),
                        balance: 0,
                    }]
                } else {
                    vec![]
                },
            },
        );
        debug!("Initialized token state account");
    }

    let (rune_receiver_account_keypair, rune_receiver_account_pubkey) = create_new_account(RUNE_RECEIVER_ACCOUNT_FILE_PATH);
    assign_ownership(rune_receiver_account_keypair, rune_receiver_account_pubkey, program_pubkey.clone());
    debug!("Created rune receiver account");
    init_rune_receiver_state_account(
        rune_receiver_account_pubkey,
        RuneReceiverState {
            account_type: AccountType::RuneReceiver,
            version: 0,
            program_state_account: submitter_pubkey,
        },
    );
    debug!("Initialized token state account");
    accounts
}

pub fn create_new_account(file_path: &str) -> (UntweakedKeypair, Pubkey) {
    let (keypair, pubkey) = with_secret_key_file(file_path)
        .expect("getting caller info should not fail");
    debug!("Creating new account {}", file_path);
    let (txid, vout) = send_utxo(pubkey.clone());
    debug!("{}:{} {:?}", txid, vout, hex::encode(pubkey));
    mine(1);

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
    debug!("create_new_account: {:?}", processed_tx);
    assert_eq!(processed_tx.status, Status::Processed);
    (keypair, pubkey)
}

pub fn assign_ownership(account_keypair: UntweakedKeypair, account_pubkey: Pubkey, program_pubkey: Pubkey) {
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
    debug!("assign_ownership: {:?}", processed_tx);
    assert_eq!(processed_tx.status, Status::Processed);

    // 10. Verify that the program is owner of caller account
    assert_eq!(
        read_account_info(NODE1_ADDRESS, account_pubkey.clone()).unwrap().owner,
        program_pubkey,
        "Program should be owner of caller account"
    );
}

pub fn init_program_state_account(
    params: InitProgramStateParams,
    expected: ProgramState,
) {
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    let (_, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
    let expected = expected.encode_to_vec().unwrap();

    debug!("Invoking contract to init program state");
    let _ = sign_and_send_instruction_success(
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

    let account = read_account_info(NODE1_ADDRESS, submitter_pubkey.clone()).unwrap();
    assert_eq!(
        expected, ProgramState::decode_from_slice(account.data.as_slice()).unwrap().encode_to_vec().unwrap()
    );

    let account = read_account_info(NODE1_ADDRESS, withdraw_pubkey.clone()).unwrap();
    let withdraw_state = WithdrawState::decode_from_slice(account.data.as_slice()).unwrap();
    assert_eq!(submitter_pubkey, withdraw_state.program_state_account);
    assert_eq!(EMPTY_HASH, withdraw_state.batch_hash);
}

pub fn init_token_state_account(
    params: InitTokenStateParams,
    token_account: Pubkey,
    expected: TokenState,
) {
    debug!("Invoking contract to init token state");
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
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
        ProgramInstruction::InitTokenState(params.clone()).encode_to_vec().unwrap(),
        vec![submitter_keypair],
    );

    let account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
    assert_eq!(
        expected.encode_to_vec().unwrap(), TokenState::decode_from_slice(account.data.as_slice()).unwrap().encode_to_vec().unwrap()
    )
}

pub fn init_rune_receiver_state_account(
    rune_receiver_account: Pubkey,
    expected: RuneReceiverState,
) {
    debug!("Invoking contract to init rune receiver state");
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    sign_and_send_instruction_success(
        vec![
            AccountMeta {
                pubkey: submitter_pubkey,
                is_signer: true,
                is_writable: true,
            },
            AccountMeta {
                pubkey: rune_receiver_account,
                is_signer: false,
                is_writable: true,
            },
        ],
        ProgramInstruction::InitRuneReceiverState().encode_to_vec().unwrap(),
        vec![submitter_keypair],
    );

    let account = read_account_info(NODE1_ADDRESS, rune_receiver_account.clone()).unwrap();
    assert_eq!(
        expected.encode_to_vec().unwrap(), RuneReceiverState::decode_from_slice(account.data.as_slice()).unwrap().encode_to_vec().unwrap()
    )
}

pub fn deposit(
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

pub fn assert_send_and_sign_deposit(
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

pub fn get_or_create_balance_index(
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

pub fn assert_send_and_sign_withdrawal(
    token_accounts: Vec<Pubkey>,
    params: WithdrawBatchParams,
    expected: Vec<TokenState>,
    expected_change_amount: Option<u64>,
    expected_events: Option<Vec<Event>>,
) {
    debug!("Performing Withdrawal");
    let wallet = CallerInfo::with_secret_key_file(WALLET1_FILE_PATH).unwrap();
    let (withdraw_keypair, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    let (rune_receiver_keypair, rune_receiver_pubkey) = with_secret_key_file(RUNE_RECEIVER_ACCOUNT_FILE_PATH).unwrap();
    let (_, program_pubkey) = with_secret_key_file(PROGRAM_FILE_PATH).unwrap();
    let program_change_address = Address::from_str(&get_account_address(program_pubkey))
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap();
    let withdraw_account_address = Address::from_str(&get_account_address(withdraw_pubkey))
        .unwrap()
        .require_network(bitcoin::Network::Regtest)
        .unwrap();

    let mut accounts = vec![
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
    ];
    if params.input_utxo_types.clone().into_iter().any(|i| i == InputUtxoType::Rune) {
        accounts.push(
            AccountMeta {
                pubkey: rune_receiver_pubkey,
                is_signer: false,
                is_writable: false,
            },
        )
    }

    token_accounts.iter().for_each(|pubkey|
        accounts.push(AccountMeta {
            pubkey: *pubkey,
            is_signer: false,
            is_writable: true,
        })
    );


    let processed_tx = sign_and_send_instruction_success(
        accounts,
        ProgramInstruction::PrepareBatchWithdraw(params.clone()).encode_to_vec().unwrap(),
        vec![submitter_keypair],
    );

    assert_eq!(processed_tx.bitcoin_txid, None);
    for i in 0..expected.len() {
        let token_state_info = read_account_info(NODE1_ADDRESS, token_accounts[i].clone()).unwrap();
        let actual = TokenState::decode_from_slice(token_state_info.data.as_slice()).unwrap();
        assert_eq!(
            expected[i].encode_to_vec().unwrap(),
            actual.encode_to_vec().unwrap(),
            "Failed comparing token state {:?} {:?}", expected[i], actual
        );
    }

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

    let mut accounts = vec![
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
    ];
    if params.input_utxo_types.clone().into_iter().any(|i| i == InputUtxoType::Rune) {
        accounts.push(
            AccountMeta {
                pubkey: rune_receiver_pubkey,
                is_signer: true,
                is_writable: false,
            }
        )
    }
    token_accounts.iter().for_each(|pubkey|
        accounts.push(
            AccountMeta {
                pubkey: *pubkey,
                is_signer: false,
                is_writable: false,
            }
        )
    );

    let processed_tx = sign_and_send_instruction_success(
        accounts,
        ProgramInstruction::SubmitBatchWithdraw(params.clone()).encode_to_vec().unwrap(),
        vec![submitter_keypair, withdraw_keypair, rune_receiver_keypair],
    );

    for i in 0..expected.len() {
        let token_state_info = read_account_info(NODE1_ADDRESS, token_accounts[i].clone()).unwrap();
        let actual = TokenState::decode_from_slice(token_state_info.data.as_slice()).unwrap();
        assert_eq!(
            expected[i].encode_to_vec().unwrap(),
            actual.encode_to_vec().unwrap(),
            "Failed comparing token state {:?} {:?}", expected[i], actual
        );
    }

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
        let mut has_rune: bool = false;

        for output in sent_tx.output.iter() {
            if output.script_pubkey == wallet.address.script_pubkey() && output.value != Amount::from_sat(DUST_THRESHOLD) {
                wallet_amount = output.value.to_sat();
            }
            if output.script_pubkey == wallet.address.script_pubkey() && output.value == Amount::from_sat(DUST_THRESHOLD) {
                has_rune = true
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
        if !has_rune {
            assert_eq!(
                params.token_withdrawals[0].withdrawals[0].amount - params.token_withdrawals[0].withdrawals[0].fee_amount,
                wallet_amount
            );
        }

        assert_eq!(
            expected_change_amount,
            change_amount
        );
        debug!("Wallet amount is {}, Change amount is {}", wallet_amount, change_amount)
    }
}

pub fn assert_send_and_sign_withdrawal_rollback(
    token_accounts: Vec<Pubkey>,
    params: RollbackWithdrawBatchParams,
    expected: Vec<TokenState>,
    has_runes: bool,
) {
    debug!("Performing Withdrawal Rollback");
    let (_, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    let (_, rune_receiver_pubkey) = with_secret_key_file(RUNE_RECEIVER_ACCOUNT_FILE_PATH).unwrap();

    let mut accounts = vec![
        AccountMeta {
            pubkey: submitter_pubkey,
            is_signer: true,
            is_writable: false,
        },
        AccountMeta {
            pubkey: withdraw_pubkey,
            is_signer: false,
            is_writable: true,
        },
    ];

    if has_runes {
        accounts.push(
            AccountMeta {
                pubkey: rune_receiver_pubkey,
                is_signer: false,
                is_writable: false,
            }
        )
    }

    token_accounts.iter().for_each(|pubkey|
        accounts.push(AccountMeta {
            pubkey: *pubkey,
            is_signer: false,
            is_writable: true,
        })
    );

    let _ = sign_and_send_instruction_success(
        accounts,
        ProgramInstruction::RollbackBatchWithdraw(params.clone()).encode_to_vec().unwrap(),
        vec![submitter_keypair],
    );

    for i in 0..expected.len() {
        let token_state_info = read_account_info(NODE1_ADDRESS, token_accounts[i].clone()).unwrap();
        let actual = TokenState::decode_from_slice(token_state_info.data.as_slice()).unwrap();
        assert_eq!(
            expected[i].encode_to_vec().unwrap(),
            actual.encode_to_vec().unwrap(),
            "Failed comparing token state {:?} {:?}", expected[i], actual
        );
    }
}

pub fn assert_send_and_sign_prepare_settlement(
    accounts: Vec<Pubkey>,
    params: SettlementBatchParams,
    expected_events: Option<Vec<Event>>,
) {
    debug!("Performing prepare Settlement Batch");
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    let (_, program_pubkey) = with_secret_key_file(PROGRAM_FILE_PATH).unwrap();


    let (txid, _) = sign_and_send_instruction(
        Instruction {
            program_id: program_pubkey,
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
    debug!("prepare_settlement: {:?}", processed_tx);
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

pub fn assert_send_and_sign_rollback_settlement() {
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

pub fn assert_send_and_sign_submit_settlement(
    program_id: Pubkey,
    accounts: Vec<Pubkey>,
    params: SettlementBatchParams,
) {
    debug!("Performing submit Settlement Batch");
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

    let (txid, _) = sign_and_send_instruction(
        Instruction {
            program_id: program_id,
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
    debug!("submit_settlement: {:?}", processed_tx);
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

pub fn update_withdraw_state_utxo() {
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
    let (withdraw_keypair, withdraw_pubkey) = with_secret_key_file(WITHDRAW_ACCOUNT_FILE_PATH).unwrap();
    let account = read_account_info(NODE1_ADDRESS, withdraw_pubkey.clone()).unwrap();
    debug!("utxo id on account is {:?}", account.utxo);

    let (new_txid, vout) = send_utxo(withdraw_pubkey.clone());

    debug!("Invoking contract to update withdraw state utxo");
    let _ = sign_and_send_instruction_success(
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
        ],
        ProgramInstruction::UpdateWithdrawStateUtxo(
            UpdateWithdrawStateUtxoParams {
                tx_id: new_txid.clone(),
                vout,
            }
        ).encode_to_vec().unwrap(),
        vec![submitter_keypair, withdraw_keypair],
    );

    let account = read_account_info(NODE1_ADDRESS, withdraw_pubkey.clone()).unwrap();
    debug!("new utxo id on account is {:?}", account.utxo);
    assert_eq!(
        format!("{}:{}", new_txid, vout), account.utxo
    );
}

pub fn set_token_rune_id(token_account: Pubkey, rune_id: String) {
    let (submitter_keypair, submitter_pubkey) = with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

    debug!("Invoking contract to update token rune id");
    let _ = sign_and_send_instruction_success(
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
        ProgramInstruction::SetTokeRuneId(
            SetTokenRuneIdParams {
                rune_id: rune_id.clone(),
            }
        ).encode_to_vec().unwrap(),
        vec![submitter_keypair],
    );

    let state_account = read_account_info(NODE1_ADDRESS, token_account.clone()).unwrap();
    let token_state: TokenState = TokenState::decode_from_slice(&state_account.data).unwrap();
    assert_eq!(
        rune_id,
        token_state.token_id
    )
}