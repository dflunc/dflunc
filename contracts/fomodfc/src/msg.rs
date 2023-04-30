use cosmwasm_std::Uint128;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct InstantiateMsg {
    pub dfc_address: String,
    pub dflunc_address: String,
    pub dev_address: String,
    pub burned_address: String,
    pub max_delay_time: u64,
    pub delay_time_per_burn: u64,
    pub initial_lunc_amount_in: Uint128,
    pub dividend_percent: u64,
    pub burned_percent: u64,
    pub invite_percent: u64,
    pub dev_percent: u64,
    pub ustc_last_fire_numerator: u64,
    pub ustc_last_fire_denominator: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ExecuteMsg {
    Burn {
        invite_address: Option<String>,
    },
    ClaimLuncDividend {
        cycle: u64,
    },
    ClaimUstcDividend {
        cycle: u64,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum QueryMsg {
    GetConfig {},
    GetCycleInfo {
        cycle: u64,
    },
    GetUserInfo {
        user_address: String,
        cycle: u64,
    },
}

// We define a custom struct for each query response
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetConfigResponse {
    pub dfc_address: String,
    pub dflunc_address: String,
    pub dev_address: String,
    pub burned_address: String,
    pub max_delay_time: u64,  // 24 hour
    pub delay_time_per_burn: u64,  // 1 minute
    pub initial_lunc_amount_in: Uint128,  // 10000 lunc
    pub dividend_percent: u64,  // 70%
    pub burned_percent: u64,      // 13%
    pub invite_percent: u64,    // 12%
    pub dev_percent: u64,       // 5%
    pub ustc_last_fire_numerator: u64,      // 2
    pub ustc_last_fire_denominator: u64,    // 3
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetBaseStateResponse {
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
    pub end_time: u64,
    pub current_cycle: u64,
    pub cycle_total_fires: u64,
    pub cycle_total_dividend: Uint128,    
    pub cycle_total_burned: Uint128,
    pub cycle_avg_lunc_dividend: Uint128,    
    pub cycle_avg_ustc_dividend: Uint128,
    pub lunc_amount_in_required: Uint128,
    pub cycle_last_burner: String,
    pub cycle_last_burner_rewards: Uint128,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct GetUserInfoResponse {
    pub user_cycle_fires: u64,
    pub user_cycle_dividend_withdrawed:Uint128,
    pub user_cycle_ustc_dividend_withdrawed: bool,
    pub user_burned_at_least_once: bool,
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
