use cosmwasm_std::{Uint128};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub dfc_address: String,
    pub lunc_batch_amount: Uint128,
    pub ustc_batch_amount: Uint128,
    pub initial_timestamp: u64,
    pub ustc_claimer_address: String,
    pub protocol_fees_reserved_rate: u64,
    pub burned_address: String,
    pub period_duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    BurnBatch {
        batch_number: u64,
    },
    ClaimRewards {
        receipt_address: String,
    },
    ClaimFees {
    },
    Stake {
        amount: Uint128,
    },
    Unstake {
        amount: Uint128,
    },
    SetUstcClaimer {
        ustc_claimer_address: String,
    },
    ClaimUstcReservedFees {
    },
    SetDfcAddress {
        dfc_address: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetConfig {},
    GetBaseState {},
    GetCycleInfo {
        cycle: u64,
    },
    GetUserInfo {
        user_address: String,
        cycle: u64,
    },
    GetAccWithdrawableStake {
        user_address: String
    },
    GetUnclaimedRewards {
        user_address: String
    },
    GetUnclaimedFees {
        user_address: String
    },
    GetCurrentCycleRewards {},
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub dfc_address: String,
    pub lunc_batch_amount: Uint128,
    pub ustc_batch_amount: Uint128,
    pub initial_timestamp: u64,
    pub ustc_claimer_address: String,
    pub owner: String,
    pub protocol_fees_reserved_rate: u64,
    pub period_duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetBaseStateResponse {
    pub current_block_time: u64,
    pub total_number_of_batches: u64,
    pub current_cycle: u64,
    pub current_started_cycle: u64,
    pub previous_started_cycle: u64,
    pub last_started_cycle: u64,
    pub pending_fees: Uint128,
    pub pending_stake: Uint128,
    pub pending_stake_withdrawal: Uint128,
    pub current_cycle_reward: Uint128,
    pub last_cycle_reward: Uint128,
    pub total_protocol_fees_reserved: Uint128,
    pub withdrawed_protocol_fees_reserved: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetCycleInfoResponse {
    pub summed_cycle_stakes: Uint128,
    pub reward_per_cycle: Uint128,
    pub cycle_total_batches_burned: u64,
    pub cycle_accrued_fees: Uint128,
    pub cycle_fees_per_stake_summed: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUserInfoResponse {
    pub acc_stake_cycle: Uint128,
    pub acc_cycle_batches_burned: u64,
    pub last_active_cycle: u64,
    pub acc_rewards: Uint128,
    pub acc_accrued_fees: Uint128,
    pub last_fee_update_cycle: u64,
    pub acc_withdrawable_stake: Uint128,
    pub acc_first_stake: u64,
    pub acc_second_stake: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetWithdrawableStakeResponse {
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUnclaimedRewardsResponse {
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetCurrentCycleRewards {
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUnclaimedFees {
    pub amount: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct MigrateMsg {}
