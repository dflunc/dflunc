use cosmwasm_std::{CanonicalAddr, Uint128, Addr};
use cw_storage_plus::{Item, Map, U64Key};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};


pub struct FomoDFCState<'a> {
    pub end_time: Item<'a, u64>,
    pub current_cycle: Item<'a, u64>,
    pub cycle_last_burner: Map<'a, U64Key, Addr>,
    pub cycle_last_burner_rewards: Map<'a, U64Key, Uint128>,
    pub cycle_total_fires: Map<'a, U64Key, u64>,
    pub cycle_total_dividend: Map<'a, U64Key, Uint128>,
    pub cycle_total_burned: Map<'a, U64Key, Uint128>,
    pub cycle_avg_lunc_dividend: Map<'a, U64Key, Uint128>,
    pub cycle_avg_ustc_dividend: Map<'a, U64Key, Uint128>,
    pub lunc_amount_in_required: Map<'a, U64Key, Uint128>,
    pub cycle_ustc_claimed: Map<'a, U64Key, bool>,

    // user withdrawable lunc = cycle_avg_lunc_dividend * user_cycle_fires - user_cycle_dividend_withdrawed
    pub user_cycle_fires: Map<'a, (Addr, U64Key), u64>,
    pub user_cycle_dividend_withdrawed: Map<'a, (Addr, U64Key), Uint128>,
    pub user_cycle_ustc_dividend_withdrawed: Map<'a, (Addr, U64Key), bool>,

    pub user_invited_address: Map<'a, Addr, Addr>,
    pub user_burned_at_least_once: Map<'a, Addr, bool>,
}

impl Default for FomoDFCState<'static> {
    fn default() -> Self {
        Self {
            end_time: Item::new("end_time"),
            current_cycle: Item::new("current_cycle"),
            cycle_last_burner: Map::new("cycle_last_burner"),
            cycle_last_burner_rewards: Map::new("cycle_last_burner_rewards"),
            cycle_total_fires: Map::new("cycle_total_fires"),
            cycle_total_dividend: Map::new("cycle_total_dividend"),
            cycle_total_burned: Map::new("cycle_total_burned"),
            cycle_avg_lunc_dividend: Map::new("cycle_avg_dividend"),
            cycle_avg_ustc_dividend: Map::new("cycle_avg_ustc_dividend"),
            lunc_amount_in_required: Map::new("lunc_amount_in_required"),
            cycle_ustc_claimed: Map::new("cycle_ustc_claimed"),
            user_cycle_fires: Map::new("user_cycle_fires"),
            user_cycle_dividend_withdrawed: Map::new("user_cycle_dividend_withdrawed"),
            user_cycle_ustc_dividend_withdrawed: Map::new("user_cycle_ustc_dividend_withdrawed"),
            user_invited_address: Map::new("user_invited_address"),
            user_burned_at_least_once: Map::new("user_burned_at_least_once"),
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct Config {
    pub dfc_address: CanonicalAddr,
    pub dflunc_address: CanonicalAddr,
    pub dev_address: CanonicalAddr,
    pub burned_address: CanonicalAddr,
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

pub const LUNC_DENOM: &str = "uluna";
pub const USTC_DENOM: &str = "uusd";

pub const CONFIG: Item<Config> = Item::new("CONFIG");

