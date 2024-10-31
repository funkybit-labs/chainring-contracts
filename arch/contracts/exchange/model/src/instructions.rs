use crate::state::{NetworkType, WalletLast4};

#[derive(Clone, PartialEq, Debug)]
pub enum ProgramInstruction {
    InitProgramState(InitProgramStateParams),
    InitTokenState(InitTokenStateParams),
    InitWalletBalances(InitWalletBalancesParams),
    BatchDeposit(DepositBatchParams),
    PrepareBatchWithdraw(WithdrawBatchParams),
    PrepareBatchSettlement(SettlementBatchParams),
    SubmitBatchSettlement(SettlementBatchParams),
    RollbackBatchSettlement(),
    RollbackBatchWithdraw(RollbackWithdrawBatchParams),
    SubmitBatchWithdraw(WithdrawBatchParams),
}

#[derive(Clone, PartialEq, Debug)]
pub struct InitProgramStateParams {
    pub fee_account: String,
    pub program_change_address: String,
    pub network_type: NetworkType,
}

#[derive(Clone, PartialEq, Debug)]
pub struct InitTokenStateParams {
    pub token_id: String,
}

#[derive(Clone, PartialEq, Debug)]
pub struct InitWalletBalancesParams {
    pub token_state_setups: Vec<TokenStateSetup>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct DepositBatchParams {
    pub token_deposits: Vec<TokenDeposits>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct WithdrawBatchParams {
    pub tx_hex: Vec<u8>,
    pub change_amount: u64,
    pub token_withdrawals: Vec<TokenWithdrawals>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SettlementBatchParams {
    pub settlements: Vec<SettlementAdjustments>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct RollbackWithdrawBatchParams {
    pub token_withdrawals: Vec<TokenWithdrawals>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct AddressIndex {
    pub index: u32,
    pub last4: WalletLast4,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Adjustment {
    pub address_index: AddressIndex,
    pub amount: u64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct TokenStateSetup {
    pub account_index: u8,
    pub wallet_addresses: Vec<String>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct Withdrawal {
    pub address_index: AddressIndex,
    pub amount: u64,
    pub fee_amount: u64,
}

#[derive(Clone, PartialEq, Debug)]
pub struct TokenDeposits {
    pub account_index: u8,
    pub deposits: Vec<Adjustment>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct TokenWithdrawals {
    pub account_index: u8,
    pub withdrawals: Vec<Withdrawal>,
}

#[derive(Clone, PartialEq, Debug)]
pub struct SettlementAdjustments {
    pub account_index: u8,
    pub increments: Vec<Adjustment>,
    pub decrements: Vec<Adjustment>,
    pub fee_amount: u64,
}

