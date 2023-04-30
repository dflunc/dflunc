#[cfg(not(feature = "library"))]
use crate::error::ContractError;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128
};

use cw2::set_contract_version;
use cw_storage_plus::U64Key;

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG, DFCState, BaseState};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:dflunc";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

impl<'a> DFCState<'a> {
    pub fn instantiate(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        msg: InstantiateMsg,
    ) -> StdResult<Response> {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        let dfc_address = deps.api.addr_canonicalize(msg.dfc_address.as_str())?;
        let ustc_claimer_address = deps.api.addr_canonicalize(msg.ustc_claimer_address.as_str())?;
        let owner = deps.api.addr_canonicalize(info.sender.as_str())?;
        let burned_address = deps.api.addr_canonicalize(msg.burned_address.as_str())?;
        CONFIG.save(
            deps.storage,
            &Config {
                dfc_address,
                lunc_batch_amount: msg.lunc_batch_amount,
                ustc_batch_amount: msg.ustc_batch_amount,
                initial_timestamp: msg.initial_timestamp,
                ustc_claimer_address,
                owner,
                protocol_fees_reserved_rate: msg.protocol_fees_reserved_rate,
                burned_address,
                period_duration: msg.period_duration,
            },
        )?;
        let init_amount = Uint128::new(100_000_000_000);        
        let mut base_state = BaseState {
            total_number_of_batches: 0,
            current_cycle: 0,
            current_started_cycle: 0,
            previous_started_cycle: 0,
            last_started_cycle: 0,
            pending_fees: Uint128::zero(),
            pending_stake: Uint128::zero(),
            pending_stake_withdrawal: Uint128::zero(),
            current_cycle_reward: Uint128::zero(),
            last_cycle_reward: Uint128::zero(),
            total_protocol_fees_reserved: Uint128::zero(),
            withdrawed_protocol_fees_reserved: Uint128::zero(),
        };
        base_state.current_cycle_reward = init_amount;
        self.base_state.save(deps.storage, &base_state)?;

        self.summed_cycle_stakes.save(deps.storage, U64Key::from(0), &init_amount)?;
        self.reward_per_cycle.save(deps.storage, U64Key::from(0), &init_amount)?;
        Ok(Response::new()
            .add_attribute("method", "instantiate")
            .add_attribute("owner", info.sender))
    }

    pub fn execute(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        msg: ExecuteMsg,
    ) -> Result<Response, ContractError> {
        match msg {
            ExecuteMsg::BurnBatch { batch_number } => self.burn_batch(deps, env, info, batch_number),
            ExecuteMsg::ClaimRewards { receipt_address } => self.claim_rewards(deps, env, info, receipt_address),
            ExecuteMsg::ClaimFees { } => self.claim_fees(deps, env, info),
            ExecuteMsg::Stake { amount } => self.stake(deps, env, info, amount),
            ExecuteMsg::Unstake {
                amount,
            } => self.unstake(deps, env, info, amount),
            ExecuteMsg::SetUstcClaimer {
                ustc_claimer_address,
            } => self.set_ustc_claimer(deps, env, info, ustc_claimer_address),
            ExecuteMsg::ClaimUstcReservedFees {  } => self.claim_ustc_reserved_fees(deps, env, info),
            ExecuteMsg::SetDfcAddress {
                dfc_address,
            } => self.set_dfc_addr(deps, env, info, dfc_address),
        }
    }
    
    pub fn query(&self, deps: Deps, env: Env, msg: QueryMsg) -> StdResult<Binary> {
        match msg {
            QueryMsg::GetConfig {  } => to_binary(&self.query_config(deps)?),
            QueryMsg::GetBaseState {  } => to_binary(&self.query_base_state(deps, env)?),
            QueryMsg::GetCurrentCycleRewards {  } => to_binary(&self.query_current_cycle_rewards(deps)?),
            QueryMsg::GetCycleInfo { cycle } => to_binary(&self.query_cycle_info(deps, cycle)?),
            QueryMsg::GetUserInfo { user_address, cycle } => to_binary(&self.query_user_info(deps, user_address, cycle)?),
            QueryMsg::GetAccWithdrawableStake { user_address } => to_binary(&self.query_acc_withdrawable_stake(deps, env, user_address)?),
            QueryMsg::GetUnclaimedRewards { user_address } => to_binary(&self.query_unclaimed_rewards(deps, env, user_address)?),
            QueryMsg::GetUnclaimedFees { user_address } => to_binary(&self.query_unclaimed_fees(deps, env, user_address)?),
        }
    }
    
    pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
        Ok(Response::default())
    }
}