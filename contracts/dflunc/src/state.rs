use cosmwasm_std::{CanonicalAddr, Uint128, Addr};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub dfc_address: CanonicalAddr,
    pub lunc_batch_amount: Uint128,
    pub ustc_batch_amount: Uint128,
    pub initial_timestamp: u64,
    pub ustc_claimer_address: CanonicalAddr,
    pub owner: CanonicalAddr,
    pub protocol_fees_reserved_rate: u64,
    pub burned_address: CanonicalAddr,
    pub period_duration: u64,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct BaseState {
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

pub struct DFCState<'a> {
    pub base_state: Item<'a, BaseState>,
    // info about cycle
    pub summed_cycle_stakes: Map<'a, U64Key, Uint128>,
    pub reward_per_cycle: Map<'a, U64Key, Uint128>,
    pub cycle_total_batches_burned: Map<'a, U64Key, u64>,
    pub cycle_accrued_fees: Map<'a, U64Key, Uint128>,
    pub cycle_fees_per_stake_summed: Map<'a, U64Key, Uint128>,

    // info about user
    pub acc_stake_cycle: Map<'a, (Addr, U64Key), Uint128>,
    pub acc_cycle_batches_burned: Map<'a, Addr, u64>,
    pub last_active_cycle: Map<'a, Addr, u64>,
    pub acc_rewards: Map<'a, Addr, Uint128>,
    pub acc_accrued_fees: Map<'a, Addr, Uint128>,
    pub last_fee_update_cycle: Map<'a, Addr, u64>,
    pub acc_withdrawable_stake: Map<'a, Addr, Uint128>,
    pub acc_first_stake: Map<'a, Addr, u64>,
    pub acc_second_stake: Map<'a, Addr, u64>,
}

impl Default for DFCState<'static> {
    fn default() -> Self {
        Self {
            base_state: Item::new("BASE_STATE"),
            summed_cycle_stakes: Map::new("SUMMED_CYCLE_STAKES"),
            reward_per_cycle: Map::new("REWARD_PER_CYCLE"),
            acc_cycle_batches_burned: Map::new("ACC_CYCLE_BATCHES_BURNED"),
            cycle_total_batches_burned: Map::new("CYCLE_TOTAL_BATCHES_BURNED"),
            last_active_cycle: Map::new("LAST_ACTIVE_CYCLE"),
            acc_rewards: Map::new("ACC_REWARDS"),
            acc_accrued_fees: Map::new("ACC_ACCRUED_FEES"),
            last_fee_update_cycle: Map::new("LAST_FEE_UPDATE_CYCLE"),
            cycle_accrued_fees: Map::new("CYCLE_ACCRUED_FEES"),
            cycle_fees_per_stake_summed: Map::new("CYCLE_FEES_PER_STAKE_SUMMED"),
            acc_stake_cycle: Map::new("ACC_STAKE_CYCLE"),
            acc_withdrawable_stake: Map::new("ACC_WITHDRAWABLE_STAKE"),
            acc_first_stake: Map::new("ACC_FIRST_STAKE"),
            acc_second_stake: Map::new("ACC_SECOND_STAKE"),
        }
    }
}

pub const LUNC_DENOM: &str = "uluna";
pub const USTC_DENOM: &str = "uusd";

pub const CONFIG: Item<Config> = Item::new("CONFIG");

