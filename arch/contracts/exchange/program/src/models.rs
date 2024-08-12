use borsh::{BorshDeserialize, BorshSerialize};

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
