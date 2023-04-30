use crate::error::ContractError;
use crate::state::{FomoDFCState, CONFIG, LUNC_DENOM, USTC_DENOM};
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, Response, Uint128, Coin, StdResult, 
    CosmosMsg, WasmMsg, to_binary, Addr, Storage, QuerierWrapper, Api, WasmQuery, QueryRequest};
use cw_storage_plus::U64Key;
use terraswap::asset::{Asset, AssetInfo};
use crate::msg::{
    GetConfigResponse, GetCycleInfoResponse, GetUserInfoResponse
};
use dflunc::msg::{ExecuteMsg as DfluncExecuteMsg, QueryMsg, GetBaseStateResponse};
use cw20::Cw20ExecuteMsg;


const MAX_BPS: u64 = 100000;

impl<'a> FomoDFCState<'a> {
    pub fn burn(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        invite_address: Option<String>,
    ) -> Result<Response, ContractError> {
    
        let config = CONFIG.load(deps.storage)?;

        let mut current_cycle = self.current_cycle.may_load(deps.storage)?.unwrap_or(0);
        let mut end_time = self.end_time.may_load(deps.storage)?.unwrap_or(0);
        let mut total_fires = self.cycle_total_fires.may_load(deps.storage, U64Key::from(current_cycle))?.unwrap_or(0);        
        let current_time = env.block.time.seconds();
        let mut messages: Vec<CosmosMsg> = vec![];
        let burn_addr = deps.api.addr_humanize(&config.burned_address)?;

        let burn_dfc_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.dfc_address)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom {
                owner: info.sender.to_string(),
                recipient: burn_addr.to_string(),
                amount: Uint128::from(1000000u128),
            })?,
            funds: vec![],
        });
        messages.push(burn_dfc_msg);

        if current_time > end_time {
            // it means the first cycle will start
            if total_fires > 0 {                  
                let dfc_base_state: GetBaseStateResponse =
                    deps.querier.query(&QueryRequest::Wasm(WasmQuery::Smart {
                        contract_addr: deps.api.addr_humanize(&config.dflunc_address)?.to_string(),
                        msg: to_binary(&QueryMsg::GetBaseState {})?,
                    }))?;
                let ustc_claimed_amount = dfc_base_state.total_protocol_fees_reserved - dfc_base_state.withdrawed_protocol_fees_reserved;
                if ustc_claimed_amount > Uint128::zero() {
                    let claim_ustc_from_dflunc_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                            contract_addr: deps.api.addr_humanize(&config.dflunc_address)?.to_string(),
                            msg: to_binary(&DfluncExecuteMsg::ClaimUstcReservedFees {
                        })?,
                        funds: vec![],
                    });
                    messages.push(claim_ustc_from_dflunc_msg);  
    
                    let messages_ustc_rewards = self.distribute_ustc(
                        &deps.querier,
                        deps.storage,
                        env.clone(),
                        ustc_claimed_amount,
                        current_cycle,
                        total_fires,
                        config.ustc_last_fire_numerator,
                        config.ustc_last_fire_denominator,
                    )?;
                    messages.extend(messages_ustc_rewards);
                }
                total_fires = 0;
                current_cycle += 1;
                self.current_cycle.save(deps.storage, &current_cycle)?;
                self.lunc_amount_in_required.save(deps.storage, U64Key::from(current_cycle), &config.initial_lunc_amount_in)?;
                self.cycle_total_fires.save(deps.storage, U64Key::from(current_cycle), &0)?;
            }

            self.end_time.save(deps.storage, &(current_time + config.max_delay_time))?;
        } else {
            end_time += config.delay_time_per_burn;
            if end_time > current_time + config.max_delay_time {
                end_time = current_time + config.max_delay_time;
            }
            self.end_time.save(deps.storage, &end_time)?;
        }

        let mut lunc_amount_in_required = self.lunc_amount_in_required.load(deps.storage, U64Key::from(current_cycle))?;
        
        let overpayment = self.validate_lunc_func(info.clone(), lunc_amount_in_required)?;
        if overpayment > Uint128::zero() {
            messages.push(self.send_lunc(&deps.querier, info.sender.clone(), overpayment)?);
        }

        let inviter_rewards = self.distribute_rewards_to_inviter(
            deps.api,
            &deps.querier,
            deps.storage,
            &mut messages,
            lunc_amount_in_required,
            invite_address,
            config.invite_percent,
        )?;

        let lunc_burned = self.burn_lunc(
            deps.as_ref(),
            &mut messages,
            lunc_amount_in_required,
            burn_addr.clone(),
            config.burned_percent,
        )?;

        self.cycle_total_burned.update(deps.storage, U64Key::from(current_cycle), |burned_lunc| -> StdResult<_> {
            Ok(burned_lunc.unwrap_or(Uint128::zero()) + lunc_burned)
        })?;

        let dev_rewards = self.distribute_dev_rewards(
            deps.as_ref(),
            &mut messages,
            lunc_amount_in_required,
            deps.api.addr_humanize(&config.dev_address)?,
            config.dev_percent,
        )?;

        let left_lunc_to_dividend = lunc_amount_in_required - inviter_rewards - lunc_burned - dev_rewards;
        if total_fires == 0 {
            messages.push(self.send_lunc(&deps.querier, burn_addr.clone(), left_lunc_to_dividend)?);

            self.user_cycle_fires.save(deps.storage, (info.sender.clone(), U64Key::from(current_cycle)), &1)?;
            self.cycle_total_burned.update(deps.storage, U64Key::from(current_cycle), |burned_lunc| -> StdResult<_> {
                Ok(burned_lunc.unwrap_or(Uint128::zero()) + left_lunc_to_dividend)
            })?;
        } else {
            self.calculate_dividend(
                deps.storage, 
                left_lunc_to_dividend, 
                info.sender.clone(), 
                total_fires, 
                current_cycle)?;
        }

        lunc_amount_in_required =  lunc_amount_in_required + Uint128::from(100000000u128);
        self.lunc_amount_in_required.save(deps.storage, U64Key::from(current_cycle), &lunc_amount_in_required)?;
        self.cycle_total_fires.update(deps.storage, U64Key::from(current_cycle), |fire| -> StdResult<_> {
            Ok(fire.unwrap_or(0) + 1)
        })?;
        self.cycle_last_burner.save(deps.storage, U64Key::from(current_cycle), &info.sender.clone())?;
        self.user_burned_at_least_once.save(deps.storage, info.sender.clone(), &true)?;
                   
        let res = Response::new()
            .add_messages(messages)
            .add_attribute("action", "burn")
            .add_attribute("burner", info.sender);
    
        Ok(res)
    }
    
    pub fn claim_lunc_dividend(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        cycle: u64,
    ) -> Result<Response, ContractError> {
        let cycle_avg_lunc_dividend = self.cycle_avg_lunc_dividend.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let user_cycle_fires = self.user_cycle_fires.may_load(deps.storage, (info.sender.clone(), U64Key::from(cycle)))?.unwrap_or(0);
        let user_cycle_dividend_withdrawed = self.user_cycle_dividend_withdrawed.may_load(deps.storage, (info.sender.clone(), U64Key::from(cycle)))?.unwrap_or(Uint128::zero());
        let user_left_lunc_dividend = cycle_avg_lunc_dividend * Uint128::from(user_cycle_fires) - user_cycle_dividend_withdrawed;

        let mut messages: Vec<CosmosMsg> = vec![];
        if user_left_lunc_dividend > Uint128::zero() {
            messages.push(self.send_lunc(&deps.querier, info.sender.clone(), user_left_lunc_dividend)?);
        }

        self.user_cycle_dividend_withdrawed.update(deps.storage, (info.sender.clone(), U64Key::from(cycle)), |dividend| -> StdResult<_> {
            Ok(dividend.unwrap_or(Uint128::zero()) + user_left_lunc_dividend)
        })?;
        let res = Response::new()
            .add_messages(messages)
            .add_attribute("action", "claim_lunc_dividend")
            .add_attribute("owner", info.sender)
            .add_attribute("amount", user_left_lunc_dividend.to_string());
    
        Ok(res)
    }
    
    pub fn claim_ustc_dividend(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        cycle: u64,
    ) -> Result<Response, ContractError> {
        let user_cycle_ustc_dividend_withdrawed = self.user_cycle_ustc_dividend_withdrawed.may_load(deps.storage, (info.sender.clone(), U64Key::from(cycle)))?.unwrap_or(false);
        if user_cycle_ustc_dividend_withdrawed {
            return Err(ContractError::AlreadyWithdrawed {cycle});
        }
        let cycle_avg_ustc_dividend = self.cycle_avg_ustc_dividend.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let user_cycle_fires = self.user_cycle_fires.may_load(deps.storage, (info.sender.clone(), U64Key::from(cycle)))?.unwrap_or(0);
        let user_left_lunc_dividend = cycle_avg_ustc_dividend * Uint128::from(user_cycle_fires);

        let mut messages: Vec<CosmosMsg> = vec![];
        if user_left_lunc_dividend > Uint128::zero() {
            let lunc_dividend = Asset {
                info: AssetInfo::NativeToken {
                    denom: USTC_DENOM.to_string(),
                },
                amount: user_left_lunc_dividend,
            };
            messages.push(lunc_dividend.into_msg(&deps.querier, deps.api.addr_validate(&info.sender.to_string())?)?);
        }

        self.user_cycle_ustc_dividend_withdrawed.save(deps.storage, (info.sender.clone(), U64Key::from(cycle)), &true)?;
        let res = Response::new()
            .add_messages(messages)
            .add_attribute("action", "claim_lunc_dividend")
            .add_attribute("owner", info.sender)
            .add_attribute("amount", user_left_lunc_dividend.to_string());
    
        Ok(res)
    }    
    
    fn distribute_ustc(
        &self,
        querier: &QuerierWrapper,
        storage: &mut dyn Storage,
        _env: Env,
        ustc_claimed_amount: Uint128,
        cycle: u64,
        total_fires: u64,
        ustc_last_fire_numerator: u64,
        ustc_last_fire_denominator: u64,
    ) -> Result<Vec<CosmosMsg>, ContractError> {
        let mut messages: Vec<CosmosMsg> = vec![];             

        // 1: 2/3 ustc to last burner
        let cycle_last_burner = self.cycle_last_burner.may_load(storage, U64Key::from(cycle))?.unwrap();
        let ustc_amount_to_last_burner = ustc_claimed_amount * Uint128::from(ustc_last_fire_numerator) / Uint128::from(ustc_last_fire_denominator);
        let ustc_rewards_to_last_burner = Asset {
            info: AssetInfo::NativeToken {
                denom: USTC_DENOM.to_string(),
            },
            amount: ustc_amount_to_last_burner,
        };
        let ustc_rewards_message = ustc_rewards_to_last_burner.into_msg(&querier, cycle_last_burner)?;
        messages.push(ustc_rewards_message);

        self.cycle_last_burner_rewards.save(storage, U64Key::from(cycle), &ustc_amount_to_last_burner)?;

        // 2: 1/3 reward was distributed equally among existing fires.
        let left_ustc_rewards = ustc_claimed_amount - ustc_amount_to_last_burner;
        let avg_ustc_rewards = left_ustc_rewards / Uint128::from(total_fires);
        self.cycle_avg_ustc_dividend.save(storage, U64Key::from(cycle), &avg_ustc_rewards)?;

        Ok(messages)
    }

    fn distribute_rewards_to_inviter(
        &self,
        api: &dyn Api,
        querier: &QuerierWrapper,
        storage: &mut dyn Storage,
        messages: &mut Vec<CosmosMsg>,
        lunc_amount_in: Uint128,
        invite_address: Option<String>,
        invite_percent: u64,
    ) -> Result<Uint128, ContractError> {
        if invite_address.is_some() {
            let invited_addr= api.addr_validate(&invite_address.unwrap())?;

            let user_burned_at_least_once = self.user_burned_at_least_once.may_load(storage, invited_addr.clone())?.unwrap_or(false);
            if !user_burned_at_least_once {
                return Ok(Uint128::zero());
            }
            
            let lunc_amount = lunc_amount_in * Uint128::from(invite_percent) / Uint128::from(MAX_BPS);
            messages.push(self.send_lunc(&querier, invited_addr, lunc_amount)?);
            return Ok(lunc_amount);
        }
        
        Ok(Uint128::zero())
    }

    fn burn_lunc(
        &self,
        deps: Deps,
        messages: &mut Vec<CosmosMsg>,
        lunc_amount_in: Uint128,
        burned_address: Addr,
        burned_percent: u64,
    ) -> Result<Uint128, ContractError> {
        let lunc_amount = lunc_amount_in * Uint128::from(burned_percent) / Uint128::from(MAX_BPS);
        messages.push(self.send_lunc(&deps.querier, burned_address, lunc_amount)?);
        return Ok(lunc_amount);
    }

    fn distribute_dev_rewards(
        &self,
        deps: Deps,
        messages: &mut Vec<CosmosMsg>,
        lunc_amount_in: Uint128,
        dev_address: Addr,
        dev_percent: u64,
    ) -> Result<Uint128, ContractError> {
        let lunc_amount = lunc_amount_in * Uint128::from(dev_percent) / Uint128::from(MAX_BPS);
        messages.push(self.send_lunc(&deps.querier, dev_address, lunc_amount)?);
        return Ok(lunc_amount);
    }

    fn send_lunc(
        &self,
        querier: &QuerierWrapper,
        receipt_address: Addr,
        lunc_amount: Uint128,
    ) -> Result<CosmosMsg, ContractError> {
        let lunc_rewards = Asset {
            info: AssetInfo::NativeToken {
                denom: LUNC_DENOM.to_string(),
            },
            amount: lunc_amount * Uint128::from(998u128) / Uint128::from(1000u128),
        };
        let message = lunc_rewards.into_msg(querier, receipt_address)?;
        Ok(message)
    }

    fn calculate_dividend(
        &self,
        storage: &mut dyn Storage,
        left_lunc_to_dividend: Uint128,
        sender: Addr,
        total_fires: u64,
        current_cycle: u64,
    ) -> Result<(), ContractError> {        
        let new_avg_lunc_per_fire = left_lunc_to_dividend / Uint128::from(total_fires);
        let mut avg_lunc_dividend_per_fire = self.cycle_avg_lunc_dividend.may_load(storage, U64Key::from(current_cycle))?.unwrap_or(Uint128::zero());
        avg_lunc_dividend_per_fire += new_avg_lunc_per_fire;
        self.cycle_avg_lunc_dividend.save(storage, U64Key::from(current_cycle), &avg_lunc_dividend_per_fire)?;

        self.user_cycle_fires.update(storage, (sender.clone(), U64Key::from(current_cycle)), |fire| -> StdResult<_> {
            Ok(fire.unwrap_or(0) + 1)
        })?;
        self.user_cycle_dividend_withdrawed.update(storage, (sender.clone(), U64Key::from(current_cycle)), |dividend| -> StdResult<_> {
            Ok(dividend.unwrap_or(Uint128::zero()) + avg_lunc_dividend_per_fire)
        })?;
        self.cycle_total_dividend.update(storage, U64Key::from(current_cycle), |dividend| -> StdResult<_> {
            Ok(dividend.unwrap_or(Uint128::zero()) + left_lunc_to_dividend)
        })?;
        Ok(())
    }

    fn validate_lunc_func(
        &self,
        info: MessageInfo,
        lunc_amount_in_required: Uint128,
    ) -> Result<Uint128, ContractError> {
        self.validate_coin_fund(info, String::from(LUNC_DENOM), lunc_amount_in_required)
    }
        
    fn validate_coin_fund(
        &self,
        info: MessageInfo,
        coin_symbol: String,
        lunc_amount_in_required: Uint128,
    ) -> Result<Uint128, ContractError> {    
        let base_fund = &Coin {
            denom: String::from(coin_symbol.as_str()),
            amount: Uint128::zero(),
        };
        let fund = info
            .funds
            .iter()
            .find(|fund| fund.denom == String::from(coin_symbol.as_str()))
            .unwrap_or(base_fund);
        
        if fund.amount < lunc_amount_in_required {
            return Err(ContractError::NotMatchedFund {
                symbol: coin_symbol,
                amount: fund.amount,
                required: lunc_amount_in_required,
            });
        }
    
        Ok(fund.amount - lunc_amount_in_required)
    }
    
   pub fn query_config(
        &self,
        deps: Deps,
    ) -> StdResult<GetConfigResponse> {
        let config = CONFIG.load(deps.storage)?;
        Ok(GetConfigResponse { 
            dfc_address: deps.api.addr_humanize(&config.dfc_address)?.to_string(),
            dflunc_address: deps.api.addr_humanize(&config.dflunc_address)?.to_string(),
            dev_address: deps.api.addr_humanize(&config.dev_address)?.to_string(),
            burned_address: deps.api.addr_humanize(&config.burned_address)?.to_string(),
            max_delay_time: config.max_delay_time,  // 24 hour
            delay_time_per_burn: config.delay_time_per_burn,  // 1 minute
            initial_lunc_amount_in: config.initial_lunc_amount_in,  // 10000 lunc
            dividend_percent: config.dividend_percent,  // 70%
            burned_percent: config.burned_percent,      // 13%
            invite_percent: config.invite_percent,    // 12%
            dev_percent: config.dev_percent,       // 5%
            ustc_last_fire_numerator: config.ustc_last_fire_numerator,      // 2
            ustc_last_fire_denominator: config.ustc_last_fire_denominator,    // 3
        })
    }
    
    pub fn query_cycle_info(&self, deps: Deps, cycle: u64) -> StdResult<GetCycleInfoResponse> {
        let end_time = self.end_time.may_load(deps.storage)?.unwrap_or(0);
        let current_cycle = self.current_cycle.may_load(deps.storage)?.unwrap_or(0);

        let cycle_total_fires = self.cycle_total_fires.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(0);
        let cycle_total_dividend = self.cycle_total_dividend.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let cycle_total_burned = self.cycle_total_burned.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let cycle_avg_lunc_dividend = self.cycle_avg_lunc_dividend.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let cycle_avg_ustc_dividend = self.cycle_avg_ustc_dividend.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let lunc_amount_in_required = self.lunc_amount_in_required.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let mut cycle_last_burner = String::from("");
        if self.cycle_last_burner.has(deps.storage, U64Key::from(cycle)) {
            cycle_last_burner = self.cycle_last_burner.load(deps.storage, U64Key::from(cycle)).unwrap().to_string();
        }
        let cycle_last_burner_rewards = self.cycle_last_burner_rewards.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        Ok(GetCycleInfoResponse { 
            end_time,
            current_cycle,
            cycle_total_fires,
            cycle_total_dividend,
            cycle_total_burned,
            cycle_avg_lunc_dividend,
            cycle_avg_ustc_dividend,
            lunc_amount_in_required,
            cycle_last_burner,
            cycle_last_burner_rewards,
        })
    }
    
    pub fn query_user_info(&self, deps: Deps, user_addr: String, cycle: u64) -> StdResult<GetUserInfoResponse> {
        let address = deps.api.addr_validate(user_addr.as_str())?;
        let user_cycle_fires = self.user_cycle_fires.may_load(deps.storage,
                                                                (address.clone(), 
                                                                 U64Key::from(cycle)))?.unwrap_or(0);
        let user_cycle_dividend_withdrawed = self.user_cycle_dividend_withdrawed.may_load(deps.storage,
                                                                (address.clone(), 
                                                                U64Key::from(cycle)))?.unwrap_or(Uint128::zero());
        let user_cycle_ustc_dividend_withdrawed = self.user_cycle_ustc_dividend_withdrawed.may_load(deps.storage,
                                                                (address.clone(), 
                                                                U64Key::from(cycle)))?.unwrap_or(false);

        let user_burned_at_least_once = self.user_burned_at_least_once.may_load(deps.storage, address.clone())?.unwrap_or(false);
        Ok(GetUserInfoResponse { 
            user_cycle_fires,
            user_cycle_dividend_withdrawed,
            user_cycle_ustc_dividend_withdrawed,
            user_burned_at_least_once
        })
    }
}
