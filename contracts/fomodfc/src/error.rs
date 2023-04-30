use thiserror::Error;
use cosmwasm_std::{StdError, Uint128};

#[derive(Error, Debug, PartialEq)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("USTC rewards in cycle-{cycle} has been claimed.")]
    CycleUstcClaimed {
        cycle: u64,
    },

    #[error("NoFireInCycle: {cycle}.")]
    NoFireInCycle {
        cycle: u64,
    },

    #[error("AlreadyWithdrawed: {cycle}.")]
    AlreadyWithdrawed {
        cycle: u64,
    },

    #[error("NotStart: start time is {initial_timestamp}.")]
    NotStart {
        initial_timestamp: u64,
    },

    #[error("NotOwner: Sender is {sender}, but owner is {owner}.")]
    NotOwner { sender: String, owner: String },

    #[error("NotClaimer: Sender is {sender}, but claimer is {claimer}.")]
    NotClaimer { sender: String, claimer: String },

    #[error("Not matched fund to execute the transaction. Symbol: {symbol}, Amount: {amount}, Required: {required}.")]
    NotMatchedFund { symbol: String, amount: Uint128, required: Uint128 },

    #[error("Batch number should be in [1, 10000].")]
    NotValidBatchNumber {
    },

    #[error("No reward.")]
    NoRewards {
    },

    #[error("No fees.")]
    NoFees {
    },

    #[error("Amount is zero.")]
    AmountIsZero {
    },

    #[error("Amount is greater than withdrawable stake.")]
    AmountGreaterThanWithdrawableStake{},
}
