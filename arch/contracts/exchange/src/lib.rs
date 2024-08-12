/// Running Tests
///
#[cfg(test)]
mod tests {
    use borsh::{BorshDeserialize, BorshSerialize};
    use common::constants::*;
    use common::helper::*;
    use common::models::*;
    use sdk::{Pubkey, UtxoMeta};
    use serial_test::serial;
    use std::str::FromStr;
    use std::{fmt, fs};
    use sha256::digest;
    use substring::Substring;

    impl fmt::Display for Balance {
        fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
            write!(f, "{} {}", self.address, self.balance)
        }
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct Balance {
        pub address: String,
        pub balance: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct TokenBalances {
        pub token_id: String,
        pub balances: Vec<Balance>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct ExchangeState {
        pub fee_account: String,
        pub last_settlement_batch_hash: String,
        pub last_withdrawal_batch_hash: String,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct Adjustment {
        pub address: String,
        pub amount: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct Withdrawal {
        pub address: String,
        pub amount: u64,
        pub fee: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub enum ExchangeInstruction {
        InitState(InitStateParams),
        Deposit(DepositParams),
        BatchWithdraw(WithdrawBatchParams),
        SubmitBatchSettlement(SettlementBatchParams),
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct InitStateParams {
        pub fee_account: String,
        pub tx_hex: Vec<u8>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct DepositParams {
        pub token: String,
        pub adjustment: Adjustment,
        pub tx_hex: Vec<u8>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct TokenWithdrawals {
        pub utxo_index: usize,
        pub withdrawals: Vec<Withdrawal>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct WithdrawBatchParams {
        pub state_utxo_index: usize,
        pub withdrawals: Vec<TokenWithdrawals>,
        pub tx_hex: Vec<u8>,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct SettlementAdjustments {
        pub utxo_index: usize,
        pub increments: Vec<Adjustment>,
        pub decrements: Vec<Adjustment>,
        pub fee_amount: u64,
    }

    #[derive(Clone, BorshSerialize, BorshDeserialize)]
    pub struct SettlementBatchParams {
        pub state_utxo_index: usize,
        pub settlements: Vec<SettlementAdjustments>,
        pub tx_hex: Vec<u8>,
    }

    use std::convert::TryInto;

    fn convert<T, const N: usize>(v: Vec<T>) -> [T; N] {
        v.try_into()
            .unwrap_or_else(|v: Vec<T>| panic!("Expected a Vec of length {} but it was {}", N, v.len()))
    }

    #[test]
    #[serial]
    fn test_program_deployed() {
        let deployed_program_id = Pubkey::from_str(&deploy_program()).unwrap();
        assert_eq!(
            fs::read("target/program.elf").expect("elf path should be available"),
            get_program(deployed_program_id.to_string())
        );
    }

    #[test]
    #[serial]
    fn test_onboard_state_utxo() {
        let deployed_program_id = Pubkey::from_str(&deploy_program()).unwrap();
        let utxos = onboard_state_utxos(deployed_program_id, "fee", 1);
        assert_eq!(
            2, utxos.len()
        );
    }

    #[test]
    #[serial]
    fn test_deposit_and_withdrawal() {
        let deployed_program_id = Pubkey::from_str(&deploy_program()).unwrap();

        let utxos = onboard_state_utxos(deployed_program_id.clone(), "fee", 1);

        let state_utxo = utxos[0].clone();
        let token_utxo = utxos[1].clone();

        let token = "btc".to_string();
        let address = "addr1".to_string();

        let (updated_token_utxo, asset_utxo_1) = deposit(
            deployed_program_id.clone(), address.clone(), token.clone(), token_utxo.clone(),10000, 10000
        );

        let (updated_token_utxo, _) = deposit(
            deployed_program_id.clone(), address.clone(), token.clone(), updated_token_utxo.clone(),6000, 16000
        );


        // perform withdrawal
        let input = WithdrawBatchParams {
            state_utxo_index: 0,
            withdrawals: vec![TokenWithdrawals {
                utxo_index: 1,
                withdrawals: vec![Withdrawal {
                    address: address.clone(),
                    amount: 5000,
                    fee: 0,
                }],
            }],
            //tx_hex: hex::decode(prepare_withdrawal(CALLER_FILE_PATH, 5000, 2000, asset_utxo_meta_1.clone())).unwrap(),
            tx_hex: hex::decode(prepare_fees()).unwrap(),
        };
        let expected = TokenBalances {
            token_id: token.clone(),
            balances: vec![
                Balance {
                    address: address.clone(),
                    balance: 11000,
                }
            ],
        };
        let _ = assert_send_and_sign_withdrawal(
            deployed_program_id.clone(),
            vec![state_utxo, updated_token_utxo, asset_utxo_1],
            input,
            expected,
        );
    }

    #[test]
    #[serial]
    fn test_settlement_submission() {
        let deployed_program_id = Pubkey::from_str(&deploy_program()).unwrap();

        let utxos = onboard_state_utxos(deployed_program_id.clone(), "fee", 2);

        let state_utxo = utxos[0].clone();
        let token1_utxo = utxos[1].clone();
        let token2_utxo = utxos[2].clone();

        let address = "addr1".to_string();

        let token1 = "btc".to_string();
        let token2 = "rune1".to_string();

        let (updated_token1_utxo, _) = deposit(
            deployed_program_id.clone(), address.clone(), token1.clone(), token1_utxo.clone(),10000, 10000
        );

        let (updated_token2_utxo, _) = deposit(
            deployed_program_id.clone(), address.clone(), token2.clone(), token2_utxo.clone(),8000, 8000
        );


        // prepare a settlement
        let input = SettlementBatchParams {
            state_utxo_index: 0,
            settlements: vec![
                SettlementAdjustments {
                    utxo_index: 1,
                    increments: vec![
                        Adjustment {
                            address: address.clone(),
                            amount: 5000,
                        }
                    ],
                    decrements: vec![],
                    fee_amount: 0,
                },
                SettlementAdjustments {
                    utxo_index: 2,
                    increments: vec![],
                    decrements: vec![
                        Adjustment {
                            address: address.clone(),
                            amount: 1000,
                        }
                    ],
                    fee_amount: 0,
                }
            ],
            tx_hex: hex::decode(prepare_fees()).unwrap(),
        };

        // now submit the settlement
        let (_updated_state_utxo, _token_utxos) = assert_send_and_sign_submit_settlement(
            deployed_program_id.clone(),
            vec![state_utxo, updated_token1_utxo.clone(), updated_token2_utxo.clone()],
            input.clone()
        );

        let token1_state = read_utxo(format!("{}:{}", _token_utxos[0].txid, _token_utxos[0].vout))
            .expect("read utxo should not fail").data;

        assert_eq!(
            borsh::to_vec(&TokenBalances {
                token_id: token1.clone(),
                balances: vec![
                    Balance {
                        address: address.clone(),
                        balance: 15000,
                    }
                ],
            }).unwrap(),
            token1_state
        );

        let token2_state = read_utxo(format!("{}:{}", _token_utxos[1].txid, _token_utxos[1].vout))
                .expect("read utxo should not fail").data;

        assert_eq!(
            borsh::to_vec(&TokenBalances {
                token_id: token2.clone(),
                balances: vec![
                    Balance {
                        address: address.clone(),
                        balance: 7000,
                    }
                ],
            }).unwrap(),
            token2_state
        );

    }

    // support functions
    fn deposit(deployed_program_id: Pubkey, address: String, token: String, token_utxo: UtxoMeta, amount: u64, expected_balance: u64) -> (UtxoMeta, UtxoMeta) {
        let input = DepositParams {
            token: token.clone(),
            adjustment: Adjustment {
                address: address.clone(),
                amount: amount,
            },
            tx_hex: hex::decode(prepare_deposit(CALLER_FILE_PATH, amount, 3000, deployed_program_id.clone())).unwrap(),
        };
        let expected = TokenBalances {
            token_id: token.clone(),
            balances: vec![
                Balance {
                    address: address.clone(),
                    balance: expected_balance,
                }
            ],
        };
        assert_send_and_sign_deposit(
            deployed_program_id.clone(),
            vec![token_utxo],
            input,
            expected,
        )
    }

    fn onboard_state_utxos(deployed_program_id: Pubkey, fee_account: &str, num_token_utxo: u32) -> Vec<UtxoMeta> {
        println!("Performing onboard state utxo");
        let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let mut utxos: Vec<UtxoMeta> = vec![];
        for i in 0..num_token_utxo + 1 {
            let state_txid = send_utxo(SUBMITTER_FILE_PATH);
            let result = read_utxo(format!("{}:1", state_txid))
                .expect("read utxo should not fail");
            print_bitcoin_tx_state(&state_txid);
            assert_eq!(
                result.authority.to_string(),
                submitter.public_key.to_string()
            );
            if i == 0 {
                utxos.push(
                    init_state_utxo(
                        deployed_program_id.clone(),
                        UtxoMeta {
                            txid: state_txid.clone(),
                            vout: 1,
                        },
                        InitStateParams {
                            fee_account: fee_account.to_string(),
                            tx_hex: hex::decode(prepare_fees()).unwrap(),
                        },
                        ExchangeState {
                            fee_account: fee_account.to_string(),
                            last_settlement_batch_hash: "".to_string(),
                            last_withdrawal_batch_hash: "".to_string(),
                        },
                    )
                )
            } else {
                utxos.push(
                    UtxoMeta {
                        txid: state_txid.clone(),
                        vout: 1,
                    }
                );
            }
        }
        for utxo in utxos.clone() {
            println!("utxo {} {}", utxo.txid, utxo.vout)
        }
        utxos
    }

    fn assert_send_and_sign_deposit(
        deployed_program_id: Pubkey,
        state_utxos: Vec<UtxoMeta>,
        params: DepositParams,
        expected: TokenBalances,
    ) -> (UtxoMeta, UtxoMeta) {
        println!("Performing Deposit");
        let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let expected = borsh::to_vec(&expected).expect("Balance should be serializable");

        let arch_network_address = get_arch_bitcoin_address();

        let input = ExchangeInstruction::Deposit(params.clone());
        let instruction_data =
            borsh::to_vec(&input).expect("ExchangeInstruction should be serializable");

        let (txid, instruction_hash) = sign_and_send_instruction(
            deployed_program_id.clone(),
            state_utxos,
            instruction_data,
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_txid = &processed_tx.bitcoin_txids[&instruction_hash];
        let utxo = read_utxo(format!("{}:0", state_txid.clone()))
            .expect("read utxo should not fail");

        print_bitcoin_tx_state(&state_txid);

        assert_eq!(
            utxo.data,
            expected,
        );

        assert_eq!(
            utxo.authority.to_string(),
            submitter.public_key.to_string(),
        );

        // get the bitcoin tx - there should be 3 outputs
        // 0 - exchanges state utxo with new state
        // 1 - OP return with the authority being set to submitter for next utxo
        // 2 - Asset Utxo sent to arch that our submitter will be authority for
        //
        let raw_tx = get_raw_transaction(state_txid);
        assert_eq!(raw_tx.output.len(), 3);
        // verify the amount of the asset utxo is expected
        assert_eq!(raw_tx.output[2].value.to_sat(), params.adjustment.amount);
        assert_eq!(raw_tx.output[2].script_pubkey, arch_network_address.script_pubkey());

        // verify the authority for the asset UTXO is our submitter
        let utxo = read_utxo(format!("{}:2", state_txid.clone()))
            .expect("read utxo should not fail");
        assert_eq!(
            utxo.authority.to_string(),
            submitter.public_key.to_string(),
        );

        (
            UtxoMeta {
                txid: state_txid.clone(),
                vout: 0,
            },
            UtxoMeta {
                txid: state_txid.clone(),
                vout: 2,
            }
        )
    }

    fn assert_send_and_sign_withdrawal(
        deployed_program_id: Pubkey,
        utxos: Vec<UtxoMeta>,
        params: WithdrawBatchParams,
        expected: TokenBalances,
    ) -> (UtxoMeta, UtxoMeta, UtxoMeta) {
        println!("Performing Withdrawal");
        let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let expected = borsh::to_vec(&expected).expect("Balance should be serializable");

        let input = ExchangeInstruction::BatchWithdraw(params.clone());
        let instruction_data =
            borsh::to_vec(&input).expect("ExchangeInstruction should be serializable");

        let (txid, instruction_hash) = sign_and_send_instruction(
            deployed_program_id.clone(),
            utxos,
            instruction_data,
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_txid = &processed_tx.bitcoin_txids[&instruction_hash];

        let state_utxo = read_utxo(format!("{}:0", state_txid.clone()))
            .expect("read utxo should not fail");
        println!("state_utxo data is {:?}", state_utxo.data);
        let exchange_state: ExchangeState = borsh::from_slice(&state_utxo.data).unwrap();

        println!("last_withdrawal_batch_hash is {}", exchange_state.last_withdrawal_batch_hash);
        assert_eq!(
            exchange_state.last_withdrawal_batch_hash,
            hash(borsh::to_vec(&params).unwrap()),
        );

        assert_eq!(
            state_utxo.authority.to_string(),
            submitter.public_key.to_string(),
        );

        let token_state = read_utxo(format!("{}:1", state_txid.clone()))
            .expect("read utxo should not fail");

        assert_eq!(
            token_state.data,
            expected,
        );

        assert_eq!(
            token_state.authority.to_string(),
            submitter.public_key.to_string(),
        );


        // get the bitcoin tx - there should be 4 outputs
        // 0 - exchanges state utxo with new state
        // 1 - uxto being sent to caller
        // 2 - OP return with the authority being set to submitter for next utxo
        // 3 - Change Asset Utxo sent to arch that our submitter will be authority for
        //
        //let raw_tx = get_raw_transaction(state_txid);
        //assert_eq!(raw_tx.output.len(), 4);
        // verify the amount of the asset utxo sent to caller matches value
        //assert_eq!(raw_tx.output[1].value.to_sat(), params.adjustments[0].amount);

        // verify the authority for the asset UTXO is our submitter
        // let utxo = read_utxo(format!("{}:3", state_txid.clone()))
        //     .expect("read utxo should not fail");
        // assert_eq!(
        //     utxo.authority.to_string(),
        //     submitter.public_key.to_string(),
        // );

        (
            UtxoMeta {
                txid: state_txid.clone(),
                vout: 0,
            },
            UtxoMeta {
                txid: state_txid.clone(),
                vout: 1,
            },
            UtxoMeta {
                txid: state_txid.clone(),
                vout: 3,
            }
        )
    }

    fn assert_send_and_sign_submit_settlement(
        deployed_program_id: Pubkey,
        utxos: Vec<UtxoMeta>,
        params: SettlementBatchParams,
    ) -> (UtxoMeta, Vec<UtxoMeta>) {
        println!("Performing submit Settlement Batch");
        let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();

        let input = ExchangeInstruction::SubmitBatchSettlement(params.clone());
        let instruction_data =
            borsh::to_vec(&input).expect("ExchangeInstruction should be serializable");

        let (txid, instruction_hash) = sign_and_send_instruction(
            deployed_program_id.clone(),
            utxos.clone(),
            instruction_data,
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_txid = &processed_tx.bitcoin_txids[&instruction_hash];

        print_bitcoin_tx_state(&state_txid);

        let state_utxo = read_utxo(format!("{}:0", state_txid.clone()))
            .expect("read utxo should not fail");

        let exchange_state: ExchangeState = borsh::from_slice(&state_utxo.data).unwrap();

        assert_eq!(
            exchange_state.last_settlement_batch_hash,
            hash(borsh::to_vec(&params).unwrap()),
        );

        assert_eq!(
            state_utxo.authority.to_string(),
            submitter.public_key.to_string(),
        );

        (
            UtxoMeta {
                txid: state_txid.clone(),
                vout: 0,
            },
            (1 .. utxos.len() as u32).map( |v| UtxoMeta {
                txid: state_txid.clone(),
                vout: v,
            }).collect()
        )
    }

    fn init_state_utxo(
        deployed_program_id: Pubkey,
        state_utxo_info: UtxoMeta,
        params: InitStateParams,
        expected: ExchangeState,
    ) -> UtxoMeta {
        let submitter = CallerInfo::with_secret_key_file(SUBMITTER_FILE_PATH).unwrap();
        let expected = borsh::to_vec(&expected).expect("Balance should be serializable");

        let input = ExchangeInstruction::InitState(params.clone());
        let instruction_data =
            borsh::to_vec(&input).expect("ExchangeInstruction should be serializable");

        let (txid, instruction_hash) = sign_and_send_instruction(
            deployed_program_id.clone(),
            vec![state_utxo_info],
            instruction_data,
        ).expect("signing and sending a transaction should not fail");

        let processed_tx = get_processed_transaction(NODE1_ADDRESS, txid)
            .expect("get processed transaction should not fail");

        let state_txid = &processed_tx.bitcoin_txids[&instruction_hash];
        let utxo = read_utxo(format!("{}:0", state_txid.clone()))
            .expect("read utxo should not fail");
        println!("utxo data is {:?}", utxo.data);

        assert_eq!(
            utxo.data,
            expected,
        );

        assert_eq!(
            utxo.authority.to_string(),
            submitter.public_key.to_string(),
        );

        UtxoMeta {
            txid: state_txid.clone(),
            vout: 0,
        }
    }

    fn hash(data: Vec<u8>) -> String {
        digest(data).substring(0, 4).to_string()
    }

    fn print_bitcoin_tx_state(txid: &str) {
        let raw_tx = get_raw_transaction(txid);
        println!("number of inputs are {}", raw_tx.input.len());
        for input in raw_tx.input.clone() {
            println!("Input - {:?}", input.previous_output)
        }

        for i in 0 .. raw_tx.output.len() {
            println!("Output: {}: {}", txid, i)
        }
    }
}
