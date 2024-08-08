use anyhow::{anyhow, Result};
use sha256::digest;
use bitcoin::{consensus, Transaction};
use crate::models::{TokenBalances, Adjustment, Balance, DepositParams, WithdrawBatchParams, SettlementBatchParams, Withdrawal, InitStateParams, ExchangeState};
use sdk::UtxoInfo;
use substring::Substring;

pub fn init_state(utxos: &[UtxoInfo],
                  params: InitStateParams) -> Result<Transaction> {
    let state_data: Vec<u8> = utxos[0].data.clone().into_inner();
    let mut state: ExchangeState = if state_data.is_empty() {
        ExchangeState {
            fee_account: "".to_string(),
            last_settlement_batch_hash: "".to_string(),
            last_withdrawal_batch_hash: "".to_string(),
        }
    } else {
        borsh::from_slice(&state_data).unwrap()
    };
    state.fee_account = params.fee_account;
    *utxos[0].data.borrow_mut() = borsh::to_vec(&state).unwrap();
    Ok(consensus::deserialize(&params.tx_hex).unwrap())
}

pub fn deposit(utxos: &[UtxoInfo],
               params: DepositParams) -> Result<Transaction> {
    let state_data: Vec<u8> = utxos[0].data.clone().into_inner();
    let existing_state: TokenBalances = if state_data.is_empty() {
        TokenBalances {
            token_id: params.token,
            balances: vec![],
        }
    } else {
        borsh::from_slice(&state_data).unwrap()
    };
    let updated_state = handle_increments(existing_state, vec![params.adjustment])?;
    *utxos[0].data.borrow_mut() = borsh::to_vec(&updated_state).unwrap();
    Ok(consensus::deserialize(&params.tx_hex).unwrap())
}

pub fn withdraw_batch(utxos: &[UtxoInfo], params: WithdrawBatchParams) -> Result<Transaction> {
    let mut existing_state = get_exchange_state(utxos, params.state_utxo_index)?;

    for token_withdrawals in &params.withdrawals {
        let token_balance_state = get_token_balance_state(utxos, token_withdrawals.utxo_index)?;
        let updated_token_balance_state = handle_withdrawals(
            token_balance_state,
            token_withdrawals.clone().withdrawals,
        )?;
        *utxos[token_withdrawals.utxo_index].data.borrow_mut() = borsh::to_vec(&updated_token_balance_state).unwrap();
    }

    existing_state.last_withdrawal_batch_hash = hash(borsh::to_vec(&params).unwrap());
    *utxos[params.state_utxo_index].data.borrow_mut() = borsh::to_vec(&existing_state).unwrap();

    Ok(consensus::deserialize(&params.tx_hex).unwrap())
}

pub fn submit_settlement_batch(utxos: &[UtxoInfo], params: SettlementBatchParams) -> Result<Transaction> {
    for token_settlements in &params.settlements {
        let token_balance_state = get_token_balance_state(utxos, token_settlements.utxo_index)?;
        let token_balance_state_1 = handle_increments(token_balance_state, token_settlements.clone().increments)?;
        let token_balance_state_2 = handle_decrements(token_balance_state_1, token_settlements.clone().decrements)?;
        *utxos[token_settlements.utxo_index].data.borrow_mut() = borsh::to_vec(&token_balance_state_2).unwrap();
    }

    let mut exchange_state = get_exchange_state(utxos, params.state_utxo_index)?;
    exchange_state.last_settlement_batch_hash = hash(borsh::to_vec(&params).unwrap());
    *utxos[params.state_utxo_index].data.borrow_mut() = borsh::to_vec(&exchange_state).unwrap();

    Ok(consensus::deserialize(&params.tx_hex).unwrap())
}

fn hash(data: Vec<u8>) -> String {
    digest(data).substring(0, 4).to_string()
}

fn get_exchange_state(utxos: &[UtxoInfo], index: usize) -> Result<ExchangeState> {
    if index >= utxos.len() {
        return Err(anyhow!("Invalid Utxo index"));
    }
    let state_data: Vec<u8> = utxos[index].data.clone().into_inner();
    Ok(borsh::from_slice(&state_data).unwrap())
}

fn get_token_balance_state(utxos: &[UtxoInfo], index: usize) -> Result<TokenBalances> {
    if index >= utxos.len() {
        return Err(anyhow!("Invalid Utxo index"));
    }
    let state_data: Vec<u8> = utxos[index].data.clone().into_inner();
    Ok(borsh::from_slice(&state_data).unwrap())
}

fn handle_increments(state: TokenBalances, adjustments: Vec<Adjustment>) -> Result<TokenBalances> {
    handle_adjustments(state, adjustments, true)
}

fn handle_decrements(state: TokenBalances, adjustments: Vec<Adjustment>) -> Result<TokenBalances> {
    handle_adjustments(state, adjustments, false)
}

fn handle_adjustments(mut state: TokenBalances, adjustments: Vec<Adjustment>, increment: bool) -> Result<TokenBalances> {
    for adjustment in adjustments {
        match state.balances.clone().into_iter().position(|b| b.address == adjustment.address) {
            Some(x) => {
                if increment {
                    state.balances[x].balance += adjustment.amount
                } else {
                    let current_balance = state.balances[x].balance;
                    let new_balance = current_balance.checked_sub(adjustment.amount);
                    state.balances[x].balance = match new_balance {
                        Some(new_balance) => new_balance,
                        None => return Err(anyhow!("Adjustment failed for {}, token{}, balance {}, adjustment{}",
                            adjustment.address, state.token_id, current_balance, adjustment.amount))
                    };
                }
            }
            None => state.balances.push(Balance {
                address: adjustment.address,
                balance: adjustment.amount,
            })
        }
    }
    Ok(state)
}

fn handle_withdrawals(mut state: TokenBalances, withdrawals: Vec<Withdrawal>) -> Result<TokenBalances> {
    for withdrawal in withdrawals {
        match state.balances.clone().into_iter().position(|b| b.address == withdrawal.address) {
            Some(x) => {
                let current_balance = state.balances[x].balance;
                let new_balance = current_balance.checked_sub(withdrawal.amount);
                state.balances[x].balance = match new_balance {
                    Some(new_balance) => new_balance,
                    None => return Err(anyhow!("Withdrawal failed for {}, token {}, balance {}, amount {}",
                            withdrawal.address, state.token_id, current_balance, withdrawal.amount))
                };
            }
            None => return Err(anyhow!("Withdrawing from wallet with no balance"))
        }
    }
    Ok(state)
}