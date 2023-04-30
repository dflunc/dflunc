#[cfg(not(feature = "library"))]
use crate::error::ContractError;
use cosmwasm_std::{
    to_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response, StdResult
};

use cw2::set_contract_version;
use cw_storage_plus::U64Key;

use crate::msg::{ExecuteMsg, InstantiateMsg, MigrateMsg, QueryMsg};
use crate::state::{Config, CONFIG, FomoDFCState};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:fomodfc";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

impl<'a> FomoDFCState<'a> {
    pub fn instantiate(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        msg: InstantiateMsg,
    ) -> StdResult<Response> {
        set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
        let dfc_address = deps.api.addr_canonicalize(msg.dfc_address.as_str())?;
        let dflunc_address = deps.api.addr_canonicalize(msg.dflunc_address.as_str())?;
        let dev_address = deps.api.addr_canonicalize(msg.dev_address.as_str())?;
        let burned_address = deps.api.addr_canonicalize(msg.burned_address.as_str())?;
        CONFIG.save(
            deps.storage,
            &Config {
                dfc_address,
                dflunc_address,
                dev_address,
                burned_address,
                max_delay_time: msg.max_delay_time,
                delay_time_per_burn: msg.delay_time_per_burn,
                initial_lunc_amount_in: msg.initial_lunc_amount_in,
                dividend_percent: msg.dividend_percent,
                burned_percent: msg.burned_percent,
                invite_percent: msg.invite_percent,
                dev_percent: msg.dev_percent,
                ustc_last_fire_numerator: msg.ustc_last_fire_numerator,
                ustc_last_fire_denominator: msg.ustc_last_fire_denominator,
            },
        )?;
        self.lunc_amount_in_required.save(deps.storage, U64Key::from(0), &msg.initial_lunc_amount_in)?;
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
            ExecuteMsg::Burn { invite_address } => self.burn(deps, env, info, invite_address),
            ExecuteMsg::ClaimLuncDividend { cycle } => self.claim_lunc_dividend(deps, env, info, cycle),
            ExecuteMsg::ClaimUstcDividend { cycle } => self.claim_ustc_dividend(deps, env, info, cycle),
        }
    }
    
    pub fn query(&self, deps: Deps, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
        match msg {
            QueryMsg::GetConfig {  } => to_binary(&self.query_config(deps)?),
            QueryMsg::GetCycleInfo { cycle } => to_binary(&self.query_cycle_info(deps, cycle)?),
            QueryMsg::GetUserInfo { user_address, cycle } => to_binary(&self.query_user_info(deps, user_address, cycle)?),
        }
    }
    
    pub fn migrate(_deps: DepsMut, _env: Env, _msg: MigrateMsg) -> StdResult<Response> {
        Ok(Response::default())
    }
}