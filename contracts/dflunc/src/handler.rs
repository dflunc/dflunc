use crate::error::ContractError;
use crate::state::{DFCState, CONFIG, LUNC_DENOM, USTC_DENOM};
use cosmwasm_std::{Deps, DepsMut, Env, MessageInfo, Response, Uint128, Coin, StdResult, StdError, 
                   CosmosMsg, WasmMsg, to_binary, Storage, Addr, CanonicalAddr, BalanceResponse, BankQuery, QueryRequest};
use cw_storage_plus::U64Key;
use terraswap::asset::{Asset, AssetInfo};
use cw20::Cw20ExecuteMsg;
use crate::msg::{
    GetConfigResponse, GetBaseStateResponse, GetCycleInfoResponse, GetUserInfoResponse, GetWithdrawableStakeResponse,
    GetUnclaimedRewardsResponse, GetCurrentCycleRewards, GetUnclaimedFees
};

fn only_owner(deps: Deps, sender: CanonicalAddr) -> Result<bool, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != config.owner {
        return Err(ContractError::NotOwner {
            sender: sender.to_string(),
            owner: deps.api.addr_humanize(&config.owner)?.to_string(),
        });
    }
    Ok(true)
}

fn only_claimer(deps: Deps, sender: CanonicalAddr) -> Result<bool, ContractError> {
    let config = CONFIG.load(deps.storage)?;
    if sender != config.ustc_claimer_address {
        return Err(ContractError::NotClaimer {
            sender: sender.to_string(),
            claimer: deps.api.addr_humanize(&config.ustc_claimer_address)?.to_string(),
        });
    }
    Ok(true)
}

const MAX_BPS: u64 = 100000;
const SCALING_FACTOR: u128 = 10_000_000_000_000;

impl<'a> DFCState<'a> {
    pub fn burn_batch(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        batch_number: u64,
    ) -> Result<Response, ContractError> {
        if batch_number > 10000 || batch_number < 1 {
            return Err(ContractError::NotValidBatchNumber {});
        }
    
        let config = CONFIG.load(deps.storage)?;

        if env.block.time.seconds() < config.initial_timestamp {
            return Err(ContractError::NotStart { initial_timestamp: config.initial_timestamp });
        }

        self.validate_lunc_func(info.clone(), batch_number, config.lunc_batch_amount)?;

        let balance_response: BalanceResponse =
            deps.querier.query(&QueryRequest::Bank(BankQuery::Balance {
                address: env.contract.address.to_string(),
                denom: LUNC_DENOM.to_string(),
            }))?;
        let total_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: balance_response.amount.denom,
            },
            amount: balance_response.amount.amount * Uint128::from(998u128) / Uint128::from(1000u128),
        };
        let message = total_asset.into_msg(&deps.querier, deps.api.addr_humanize(&config.burned_address)?)?;

        let protocol_fee_per_batch = (config.ustc_batch_amount * Uint128::from(MAX_BPS - 5 * batch_number)) / Uint128::from(MAX_BPS);
        self.validate_ustc_func(info.clone(), batch_number, protocol_fee_per_batch)?;
        
        self.calculate_cycle(deps.storage, env.block.time.seconds())?;
        self.update_cycle_fees_per_stake_summed(deps.storage)?;
        self.set_up_new_cycle(deps.storage)?;
        self.update_stats(deps.storage, info.sender.clone())?;

        let mut base_state = self.base_state.load(deps.storage)?;
        self.last_active_cycle.save(deps.storage, info.sender.clone(), &base_state.current_cycle)?;

        base_state.total_number_of_batches += batch_number;           
        self.cycle_total_batches_burned.update(deps.storage,
                                               U64Key::from(base_state.current_cycle),
                                               |burned_batchs: Option<u64>| -> StdResult<_> {
                                                Ok(burned_batchs.unwrap_or_default() + batch_number)
                                            })?;
        self.acc_cycle_batches_burned.update(deps.storage, 
                                            info.sender.clone(), 
                                            |burned_batchs: Option<u64>| -> StdResult<_> {
                                                Ok(burned_batchs.unwrap_or_default() + batch_number)
                                            })?;

        let protocol_fee_reserved = protocol_fee_per_batch * Uint128::from(batch_number) * Uint128::from(config.protocol_fees_reserved_rate) / Uint128::from(MAX_BPS);
        self.cycle_accrued_fees.update(deps.storage, 
                                       U64Key::from(base_state.current_cycle), 
                                       |accrued_fees: Option<Uint128>| -> StdResult<_> {
                                            Ok(accrued_fees.unwrap_or_default().checked_add(
                                                protocol_fee_per_batch * Uint128::from(batch_number) - protocol_fee_reserved)?)
                                        })?;    
        base_state.total_protocol_fees_reserved += protocol_fee_reserved;                                    

        self.base_state.save(deps.storage, &base_state)?;                        
        let res = Response::new()
            .add_message(message)
            .add_attribute("action", "burnBatch")
            .add_attribute("burner", info.sender)
            .add_attribute("batch_number", batch_number.to_string());
    
        Ok(res)
    }
    
    pub fn claim_rewards(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        receipt_address: String,
    ) -> Result<Response, ContractError> {
    
        self.calculate_cycle(deps.storage, env.block.time.seconds())?;
        self.update_cycle_fees_per_stake_summed(deps.storage)?;
        self.update_stats(deps.storage, info.sender.clone())?;

        let acc_rewards = self.acc_rewards.may_load(deps.storage, info.sender.clone())?.unwrap_or(Uint128::zero());
        let acc_withdrawable_stake = self.acc_withdrawable_stake.may_load(deps.storage, info.sender.clone())?.unwrap_or(Uint128::zero());
        let reward = acc_rewards - acc_withdrawable_stake;
        
        if reward.is_zero() {
            return Err(ContractError::NoRewards {});
        }

        self.acc_rewards.update(
            deps.storage,
            info.sender.clone(),
            |reward_before: Option<Uint128>| -> StdResult<_> {
                Ok(reward_before.unwrap_or_default().checked_sub(reward)?)
            },
        )?;

        let mut base_state = self.base_state.load(deps.storage)?;
        if base_state.last_started_cycle == base_state.current_started_cycle {
            base_state.pending_stake_withdrawal += reward;
        } else {
            self.summed_cycle_stakes.update(
                deps.storage,
                U64Key::from(base_state.current_cycle),
                |summed_cycle_stakes_before: Option<Uint128>| -> StdResult<_> {
                    Ok(summed_cycle_stakes_before.unwrap_or_default().checked_sub(reward)?)
                },
            )?;
        }

        let mut messages: Vec<CosmosMsg> = vec![];
        let config = CONFIG.load(deps.storage)?;
        let mint_dfc_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.dfc_address)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Mint {
                recipient: receipt_address.clone(),
                amount: reward,
            })?,
            funds: vec![],
        });
        messages.push(mint_dfc_msg);

        self.base_state.save(deps.storage, &base_state)?;

        let res = Response::new()
            .add_messages(messages)
            .add_attribute("action", "claimRewards")
            .add_attribute("owner", info.sender)
            .add_attribute("receipt", receipt_address)
            .add_attribute("amount", reward.to_string());
    
        Ok(res)
    }
    
    pub fn claim_fees(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
    ) -> Result<Response, ContractError> {
    
        self.calculate_cycle(deps.storage, env.block.time.seconds())?;
        self.update_cycle_fees_per_stake_summed(deps.storage)?;
        self.update_stats(deps.storage, info.sender.clone())?;

        let fees = self.acc_accrued_fees.may_load(deps.storage, info.sender.clone())?.unwrap_or(Uint128::zero());
        if fees.is_zero() {
            return Err(ContractError::NoFees {});
        }
        self.acc_accrued_fees.save(deps.storage, info.sender.clone(), &Uint128::zero())?;

        let total_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: USTC_DENOM.to_string(),
            },
            amount: fees,
        };
        let message = total_asset.into_msg(&deps.querier, info.sender.clone());
        
        let res = Response::new()
            .add_message(message?)
            .add_attribute("action", "claimFees")
            .add_attribute("claimer", info.sender);
    
        Ok(res)
    }
    
    pub fn stake(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        amount: Uint128
    ) -> Result<Response, ContractError> {
        if amount == Uint128::zero() {
            return Err(ContractError::AmountIsZero {});
        }

        self.calculate_cycle(deps.storage, env.block.time.seconds())?;
        self.update_cycle_fees_per_stake_summed(deps.storage)?;
        self.update_stats(deps.storage, info.sender.clone())?;

        let mut base_state = self.base_state.load(deps.storage)?;
        base_state.pending_stake += amount;
        let mut cycle_to_set = base_state.current_cycle + 1;

        if base_state.last_started_cycle == base_state.current_started_cycle {
            cycle_to_set = base_state.last_started_cycle + 1;
        }

        let acc_first_stake = self.acc_first_stake.may_load(deps.storage, info.sender.clone())?.unwrap_or(0);
        let acc_second_stake = self.acc_second_stake.may_load(deps.storage, info.sender.clone())?.unwrap_or(0);
        if cycle_to_set != acc_first_stake && cycle_to_set != acc_second_stake {
            if acc_second_stake == 0 {
                self.acc_first_stake.save(deps.storage, info.sender.clone(), &cycle_to_set)?;
            } else if acc_second_stake == 0 {
                self.acc_second_stake.save(deps.storage, info.sender.clone(), &cycle_to_set)?;
            }
        }
        self.acc_stake_cycle.update(
            deps.storage,
            (info.sender.clone(), U64Key::from(cycle_to_set)),
            |acc_stake_cycle_before: Option<Uint128>| -> StdResult<_> {
                Ok(acc_stake_cycle_before.unwrap_or_default() + amount)
            },
        )?;

        let mut messages: Vec<CosmosMsg> = vec![];
        let config = CONFIG.load(deps.storage)?;
        let transfer_from_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.dfc_address)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::TransferFrom { 
                owner: info.sender.to_string(), 
                recipient: env.contract.address.to_string(), 
                amount,
            })?,
            funds: vec![],
        });
        messages.push(transfer_from_msg);

        self.base_state.save(deps.storage, &base_state)?;
    
        let res = Response::new()
            .add_messages(messages)
            .add_attribute("action", "stake")
            .add_attribute("staker", info.sender)
            .add_attribute("amount", amount.to_string());
    
        Ok(res)
    }
    
    pub fn unstake(
        &self,
        deps: DepsMut,
        env: Env,
        info: MessageInfo,
        amount: Uint128
    ) -> Result<Response, ContractError> {
        if amount == Uint128::zero() {
            return Err(ContractError::AmountIsZero {});
        }

        self.calculate_cycle(deps.storage, env.block.time.seconds())?;
        self.update_cycle_fees_per_stake_summed(deps.storage)?;
        self.update_stats(deps.storage, info.sender.clone())?;
        
        let acc_withdrawable_stake = self.acc_withdrawable_stake.may_load(deps.storage, info.sender.clone())?.unwrap_or(Uint128::zero());
        if amount > acc_withdrawable_stake {
            return Err(ContractError::AmountGreaterThanWithdrawableStake {});
        }
        let mut base_state = self.base_state.load(deps.storage)?;
        if base_state.last_started_cycle == base_state.current_started_cycle {
            base_state.pending_stake_withdrawal += amount;
        } else {
            self.summed_cycle_stakes.update(
                deps.storage,
                U64Key::from(base_state.current_cycle),
                |summed_cycle_stake_before: Option<Uint128>| -> StdResult<_> {
                    Ok(summed_cycle_stake_before.unwrap_or_default() - amount)
                },
            )?;
        }
        self.acc_withdrawable_stake.update(deps.storage, 
                                           info.sender.clone(), 
                                           |stake: Option<Uint128>| -> StdResult<_> {
                                                Ok(stake.unwrap_or_default().checked_sub(amount)?)
                                           })?;
        self.acc_rewards.update(deps.storage, 
                                info.sender.clone(),
                                |reward: Option<Uint128>| -> StdResult<_> {
                                    Ok(reward.unwrap_or_default().checked_sub(amount)?)
                                })?;                               


        let mut messages: Vec<CosmosMsg> = vec![];
        let config = CONFIG.load(deps.storage)?;
        let transfer_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: deps.api.addr_humanize(&config.dfc_address)?.to_string(),
            msg: to_binary(&Cw20ExecuteMsg::Transfer { 
                recipient: info.sender.to_string(), 
                amount,
            })?,
            funds: vec![],
        });
        messages.push(transfer_msg);

        self.base_state.save(deps.storage, &base_state)?;

        let res = Response::new()
            .add_messages(messages)
            .add_attribute("action", "unstake")
            .add_attribute("staker", info.sender)
            .add_attribute("amount", amount.to_string());
    
        Ok(res)
    }
    
    pub fn set_ustc_claimer(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        ustc_claimer: String
    ) -> Result<Response, ContractError> {
        let sender = deps.api.addr_canonicalize(info.sender.as_str())?;
        only_owner(deps.as_ref(), sender)?;
        
        let mut config = CONFIG.load(deps.storage)?;
        config.ustc_claimer_address = deps.api.addr_canonicalize(ustc_claimer.as_str())?;
        CONFIG.save(deps.storage, &config)?;
    
        let res = Response::new()
            .add_attribute("action", "setUstcClaimer")
            .add_attribute("ustc_claimer", ustc_claimer);
    
        Ok(res)
    }

    pub fn set_dfc_addr(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
        dfc_addr: String
    ) -> Result<Response, ContractError> {
        let sender = deps.api.addr_canonicalize(info.sender.as_str())?;
        only_owner(deps.as_ref(), sender)?;
        
        let mut config = CONFIG.load(deps.storage)?;
        config.dfc_address = deps.api.addr_canonicalize(dfc_addr.as_str())?;
        CONFIG.save(deps.storage, &config)?;
    
        let res = Response::new()
            .add_attribute("action", "setUstcClaimer")
            .add_attribute("ustc_claimer", dfc_addr);
    
        Ok(res)
    }

    pub fn claim_ustc_reserved_fees(
        &self,
        deps: DepsMut,
        _env: Env,
        info: MessageInfo,
    ) -> Result<Response, ContractError> {
        let sender = deps.api.addr_canonicalize(info.sender.as_str())?;
        only_claimer(deps.as_ref(), sender)?;
        
        let mut base_state = self.base_state.load(deps.storage)?;
        let claimable_ustc_amount = base_state.total_protocol_fees_reserved - base_state.withdrawed_protocol_fees_reserved;

        let total_asset = Asset {
            info: AssetInfo::NativeToken {
                denom: USTC_DENOM.to_string(),
            },
            amount: claimable_ustc_amount,
        };
        let message = total_asset.into_msg(&deps.querier, info.sender.clone())?;

        base_state.withdrawed_protocol_fees_reserved = base_state.total_protocol_fees_reserved;
        self.base_state.save(deps.storage, &base_state)?;
    
        let res = Response::new()
            .add_message(message)
            .add_attribute("action", "claimUstcReservedFees")
            .add_attribute("ustc_claimer", info.sender.to_string())
            .add_attribute("ustc_amount", claimable_ustc_amount.to_string());
    
        Ok(res)
    }
        
    fn validate_lunc_func(
        &self,
        info: MessageInfo,
        batch_number: u64,
        lunc_batch_amount: Uint128,
    ) -> Result<(), ContractError> {
        self.validate_burn_fund(info, String::from(LUNC_DENOM), batch_number, lunc_batch_amount)
    }
    
    fn validate_ustc_func(
        &self,
        info: MessageInfo,
        batch_number: u64,
        protocol_fee_per_batch: Uint128,
    ) -> Result<(), ContractError> {
        self.validate_burn_fund(info, String::from(USTC_DENOM), batch_number, protocol_fee_per_batch)
    }
    
    fn validate_burn_fund(
        &self,
        info: MessageInfo,
        coin_symbol: String,
        batch_number: u64,
        amount_per_batch: Uint128,
    ) -> Result<(), ContractError> {    
        let base_fund = &Coin {
            denom: String::from(coin_symbol.as_str()),
            amount: Uint128::zero(),
        };
        let fund = info
            .funds
            .iter()
            .find(|fund| fund.denom == String::from(coin_symbol.as_str()))
            .unwrap_or(base_fund);
        
        let fund_amount_required = Uint128::from(batch_number) * amount_per_batch;
        if fund.amount != fund_amount_required {
            return Err(ContractError::NotMatchedFund {
                symbol: coin_symbol,
                amount: fund.amount,
                required: fund_amount_required,
            });
        }
    
        Ok(())
    }
    
    fn get_current_cycle(&self, storage: &mut dyn Storage, current_block_time: u64) -> StdResult<u64> {
        let config = CONFIG.load(storage)?;
    
        let elapsed_time = current_block_time
            .checked_sub(config.initial_timestamp)
            .ok_or_else(|| StdError::generic_err("Invalid elapsed time"))?;
    
        let calculated_cycle = elapsed_time / config.period_duration;
    
        Ok(calculated_cycle)
    }
    
    fn calculate_cycle(&self, storage: &mut dyn Storage, current_block_time: u64) -> StdResult<Response> {
        let calculated_cycle = self.get_current_cycle(storage, current_block_time)?;
        let mut base_state = self.base_state.load(storage)?;
    
        if calculated_cycle > base_state.current_cycle {
            base_state.current_cycle = calculated_cycle;
            self.base_state.save(storage, &base_state)?;
        }
    
        Ok(Response::default())
    }
    
    fn update_cycle_fees_per_stake_summed(&self, storage: &mut dyn Storage) -> StdResult<Response> {
        let mut base_state = self.base_state.load(storage)?;
    
        if base_state.current_cycle != base_state.current_started_cycle {        
            base_state.previous_started_cycle = base_state.last_started_cycle + 1;
            base_state.last_started_cycle = base_state.current_started_cycle;    
        }
        
        let last_cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(base_state.last_started_cycle + 1))?.unwrap_or(Uint128::zero());
        
        if base_state.current_cycle > base_state.last_started_cycle && last_cycle_fees_per_stake_summed == Uint128::zero() {
            let last_summed_cycle_stakes = self.summed_cycle_stakes.may_load(storage, U64Key::from(base_state.last_started_cycle))?.unwrap_or(Uint128::zero());
            
            let mut fee_per_stake = Uint128::zero();
            if last_summed_cycle_stakes != Uint128::zero() {
                let last_cycle_accrued_fees = self.cycle_accrued_fees.may_load(storage, U64Key::from(base_state.last_started_cycle))?.unwrap_or(Uint128::zero());
                fee_per_stake = ((last_cycle_accrued_fees + base_state.pending_fees) * Uint128::from(SCALING_FACTOR)) / last_summed_cycle_stakes;
                base_state.pending_fees = Uint128::zero();
            } else {
                let last_cycle_accrued_fees = self.cycle_accrued_fees.may_load(storage, U64Key::from(base_state.last_started_cycle))?.unwrap_or(Uint128::zero());
                base_state.pending_fees += last_cycle_accrued_fees;
                fee_per_stake = Uint128::zero();
            }
            
            let previous_cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(base_state.previous_started_cycle))?.unwrap_or(Uint128::zero());        
            self.cycle_fees_per_stake_summed.save(storage, U64Key::from(base_state.last_started_cycle + 1), 
                                                                    &(previous_cycle_fees_per_stake_summed + fee_per_stake))?;
        }

        self.base_state.save(storage, &base_state)?;
        Ok(Response::default())
    }
    
    fn set_up_new_cycle(&self, storage: &mut dyn Storage) -> StdResult<Response> {
        let mut base_state = self.base_state.load(storage)?;
        let reward_per_cycle = self.reward_per_cycle.may_load(storage, U64Key::from(base_state.current_cycle))?.unwrap_or(Uint128::zero());
        if reward_per_cycle == Uint128::zero() {
            base_state.last_cycle_reward = base_state.current_cycle_reward;
            let calculated_cycle_reward = (base_state.last_cycle_reward * Uint128::from(10000u128)) / Uint128::from(10020u128);
            base_state.current_cycle_reward = calculated_cycle_reward;
            self.reward_per_cycle.save(storage, U64Key::from(base_state.current_cycle), &calculated_cycle_reward)?;
    
            base_state.current_started_cycle = base_state.current_cycle;
            
            let last_summed_cycle_stakes = self.summed_cycle_stakes.may_load(storage, U64Key::from(base_state.last_started_cycle))?.unwrap_or(Uint128::zero());
            let mut new_summed_cycle_stakes = last_summed_cycle_stakes + base_state.current_cycle_reward;
            
            if base_state.pending_stake != Uint128::zero() {
                new_summed_cycle_stakes += base_state.pending_stake;
                base_state.pending_stake = Uint128::zero();
            }
            
            if base_state.pending_stake_withdrawal != Uint128::zero() {
                new_summed_cycle_stakes -= base_state.pending_stake_withdrawal;
                base_state.pending_stake_withdrawal = Uint128::zero();
            }
            self.summed_cycle_stakes.save(storage, U64Key::from(base_state.current_started_cycle), &new_summed_cycle_stakes)?;
            self.base_state.save(storage, &base_state)?;
        }

        Ok(Response::default())
    }

    fn update_stats(&self, storage: &mut dyn Storage, user_addr: Addr) -> StdResult<Response> {
        let base_state = self.base_state.load(storage)?;
        
        let user_last_active_cycle = self.last_active_cycle.may_load(storage, user_addr.clone())?.unwrap_or(0);
        let user_acc_cycle_batches_burned = self.acc_cycle_batches_burned.may_load(storage, user_addr.clone())?.unwrap_or(0);
        
        if base_state.current_cycle > user_last_active_cycle && user_acc_cycle_batches_burned != 0 {	
            let reward_per_cycle = self.reward_per_cycle.may_load(storage, U64Key::from(user_last_active_cycle))?.unwrap_or(Uint128::zero());
            let cycle_total_batches_burned = self.cycle_total_batches_burned.may_load(storage, U64Key::from(user_last_active_cycle))?.unwrap_or(0);
            let last_cycle_acc_reward = (Uint128::from(user_acc_cycle_batches_burned) * reward_per_cycle) / Uint128::from(cycle_total_batches_burned);	
            self.acc_rewards.update(
                storage,
                user_addr.clone(),
                |reward: Option<Uint128>| -> StdResult<_> {
                    Ok(reward.unwrap_or_default().checked_add(last_cycle_acc_reward)?)
                },
            )?;
            self.acc_cycle_batches_burned.save(storage, user_addr.clone(), &0)?;
       }

       let user_last_fee_update_cycle = self.last_fee_update_cycle.may_load(storage, user_addr.clone())?.unwrap_or(0);
       if base_state.current_cycle > base_state.last_started_cycle && user_last_fee_update_cycle != base_state.last_started_cycle + 1 {
            let acc_rewards = self.acc_rewards.may_load(storage, user_addr.clone())?.unwrap_or(Uint128::zero());
            let cycle_fees_per_stake_summed_1 = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(base_state.last_started_cycle + 1))?.unwrap_or(Uint128::zero());
            let cycle_fees_per_stake_summed_2 = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(user_last_fee_update_cycle))?.unwrap_or(Uint128::zero());
            self.acc_accrued_fees.update(
                storage,
                user_addr.clone(),
                |fees: Option<Uint128>| -> StdResult<_> {
                    let added_fees = acc_rewards * (cycle_fees_per_stake_summed_1 - cycle_fees_per_stake_summed_2) / Uint128::from(SCALING_FACTOR);
                    Ok(fees.unwrap_or_default().checked_add(added_fees)?)
                },
            )?;
            self.last_fee_update_cycle.save(storage, user_addr.clone(), &(base_state.last_started_cycle + 1))?;
        }

        let acc_first_stake = self.acc_first_stake.may_load(storage, user_addr.clone())?.unwrap_or(0);
        if acc_first_stake != 0 && base_state.current_cycle > acc_first_stake {
            let unlocked_first_stake = self.acc_stake_cycle.may_load(storage, (user_addr.clone(), U64Key::from(acc_first_stake)))?.unwrap_or(Uint128::zero());
            self.acc_rewards.update(
                storage,
                user_addr.clone(),
                |reward: Option<Uint128>| -> StdResult<_> {
                    Ok(reward.unwrap_or_default().checked_add(unlocked_first_stake)?)
                },
            )?;
            self.acc_withdrawable_stake.update(
                storage,
                user_addr.clone(),
                |stake: Option<Uint128>| -> StdResult<_> {
                    Ok(stake.unwrap_or_default().checked_add(unlocked_first_stake)?)
                },
            )?;
            if base_state.last_started_cycle + 1 > acc_first_stake {
                let cycle_fees_per_stake_summed_1 = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(base_state.last_started_cycle + 1))?.unwrap_or(Uint128::zero());
                let cycle_fees_per_stake_summed_2 = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(acc_first_stake))?.unwrap_or(Uint128::zero());
                self.acc_accrued_fees.update(
                    storage,
                    user_addr.clone(),
                    |fees: Option<Uint128>| -> StdResult<_> {
                        let added_fees = (unlocked_first_stake * (cycle_fees_per_stake_summed_1 - cycle_fees_per_stake_summed_2)) / Uint128::from(SCALING_FACTOR);
                        Ok(fees.unwrap_or_default().checked_add(added_fees)?)
                    },
                )?;
            }
            self.acc_stake_cycle.save(storage, (user_addr.clone(), U64Key::from(acc_first_stake)), &Uint128::zero())?;
            self.acc_first_stake.save(storage, user_addr.clone(), &0)?;

            let acc_second_stake = self.acc_second_stake.may_load(storage, user_addr.clone())?.unwrap_or(0);
            if acc_second_stake != 0 {                
                if base_state.current_cycle > acc_second_stake {
                    let unlocked_second_stake = self.acc_stake_cycle.may_load(storage, (user_addr.clone(), U64Key::from(acc_second_stake)))?.unwrap_or(Uint128::zero());
                    self.acc_rewards.update(
                        storage,
                        user_addr.clone(),
                        |reward: Option<Uint128>| -> StdResult<_> {
                            Ok(reward.unwrap_or_default().checked_add(unlocked_second_stake)?)
                        },
                    )?;
                    self.acc_withdrawable_stake.update(
                        storage,
                        user_addr.clone(),
                        |stake: Option<Uint128>| -> StdResult<_> {
                            Ok(stake.unwrap_or_default().checked_add(unlocked_second_stake)?)
                        },
                    )?;
                    if base_state.last_started_cycle + 1 > acc_second_stake {
                        let cycle_fees_per_stake_summed_1 = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(base_state.last_started_cycle + 1))?.unwrap_or(Uint128::zero());
                        let cycle_fees_per_stake_summed_2 = self.cycle_fees_per_stake_summed.may_load(storage, U64Key::from(acc_second_stake))?.unwrap_or(Uint128::zero());
                        self.acc_accrued_fees.update(
                            storage,
                            user_addr.clone(),
                            |fees: Option<Uint128>| -> StdResult<_> {
                                let added_fees = (unlocked_second_stake * (cycle_fees_per_stake_summed_1 - cycle_fees_per_stake_summed_2)) / Uint128::from(SCALING_FACTOR);
                                Ok(fees.unwrap_or_default().checked_add(added_fees)?)
                            },
                        )?;
                        self.acc_stake_cycle.save(storage, (user_addr.clone(), U64Key::from(acc_second_stake)), &Uint128::zero())?;
                        self.acc_second_stake.save(storage, user_addr.clone(), &0)?;
                    } else {
                        self.acc_first_stake.save(storage, user_addr.clone(), &acc_second_stake)?;
                        self.acc_second_stake.save(storage, user_addr.clone(), &0)?;
                    }
                } 
            }
        }
        Ok(Response::default())
   }

   pub fn query_config(
        &self,
        deps: Deps,
    ) -> StdResult<GetConfigResponse> {
        let config = CONFIG.load(deps.storage)?;
        Ok(GetConfigResponse { 
            dfc_address: deps.api.addr_humanize(&config.dfc_address)?.to_string(),
            lunc_batch_amount: config.lunc_batch_amount,
            ustc_batch_amount: config.ustc_batch_amount,
            initial_timestamp: config.initial_timestamp,
            ustc_claimer_address: deps.api.addr_humanize(&config.ustc_claimer_address)?.to_string(),
            owner: deps.api.addr_humanize(&config.owner)?.to_string(),
            protocol_fees_reserved_rate: config.protocol_fees_reserved_rate,
            period_duration: config.period_duration,
        })
    }

    pub fn query_base_state(
        &self,
        deps: Deps,
        env: Env,
    ) -> StdResult<GetBaseStateResponse> {
        let base_state = self.base_state.load(deps.storage)?;
        Ok(GetBaseStateResponse { 
            current_block_time: env.block.time.seconds(),
            total_number_of_batches: base_state.total_number_of_batches,
            current_cycle: base_state.current_cycle,
            current_started_cycle: base_state.current_started_cycle,
            previous_started_cycle: base_state.previous_started_cycle,
            last_started_cycle: base_state.last_started_cycle,
            pending_fees: base_state.pending_fees,
            pending_stake: base_state.pending_stake,
            pending_stake_withdrawal: base_state.pending_stake_withdrawal,
            current_cycle_reward: base_state.current_cycle_reward,
            last_cycle_reward: base_state.last_cycle_reward,
            total_protocol_fees_reserved: base_state.total_protocol_fees_reserved,
            withdrawed_protocol_fees_reserved: base_state.withdrawed_protocol_fees_reserved,
        })
    }
    
    pub fn query_cycle_info(&self, deps: Deps, cycle: u64) -> StdResult<GetCycleInfoResponse> {
        let summed_cycle_stakes = self.summed_cycle_stakes.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let reward_per_cycle = self.reward_per_cycle.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let cycle_total_batches_burned = self.cycle_total_batches_burned.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(0);
        let cycle_accrued_fees = self.cycle_accrued_fees.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        let cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(deps.storage, U64Key::from(cycle))?.unwrap_or(Uint128::zero());
        Ok(GetCycleInfoResponse { 
            summed_cycle_stakes,
            reward_per_cycle,
            cycle_total_batches_burned,
            cycle_accrued_fees,
            cycle_fees_per_stake_summed,
        })
    }
    
    pub fn query_user_info(&self, deps: Deps, user_addr: String, cycle: u64) -> StdResult<GetUserInfoResponse> {
        let address = deps.api.addr_validate(user_addr.as_str())?;
        let acc_stake_cycle = self.acc_stake_cycle.may_load(deps.storage,
                                                                (address.clone(), 
                                                                 U64Key::from(cycle)))?.unwrap_or(Uint128::zero());

        let acc_cycle_batches_burned = self.acc_cycle_batches_burned.may_load(deps.storage, address.clone())?.unwrap_or(0);
        let last_active_cycle = self.last_active_cycle.may_load(deps.storage, address.clone())?.unwrap_or(0);
        let acc_rewards = self.acc_rewards.may_load(deps.storage, address.clone())?.unwrap_or(Uint128::zero());
        let acc_accrued_fees = self.acc_accrued_fees.may_load(deps.storage, address.clone())?.unwrap_or(Uint128::zero());
        let last_fee_update_cycle = self.last_fee_update_cycle.may_load(deps.storage, address.clone())?.unwrap_or(0);
        let acc_withdrawable_stake = self.acc_withdrawable_stake.may_load(deps.storage, address.clone())?.unwrap_or(Uint128::zero());
        let acc_first_stake = self.acc_first_stake.may_load(deps.storage, address.clone())?.unwrap_or(0);
        let acc_second_stake = self.acc_second_stake.may_load(deps.storage, address.clone())?.unwrap_or(0);
        Ok(GetUserInfoResponse { 
            acc_stake_cycle,
            acc_cycle_batches_burned,
            last_active_cycle,
            acc_rewards,
            acc_accrued_fees,
            last_fee_update_cycle,
            acc_withdrawable_stake,
            acc_first_stake,
            acc_second_stake,
        })
    }

    pub fn query_acc_withdrawable_stake(&self, deps: Deps, env: Env, user_addr: String) -> StdResult<GetWithdrawableStakeResponse> {
        let config = CONFIG.load(deps.storage)?;
    
        let elapsed_time = env.block.time.seconds()
            .checked_sub(config.initial_timestamp)
            .ok_or_else(|| StdError::generic_err("Invalid elapsed time"))?;
    
        let calculated_cycle = elapsed_time / config.period_duration;

        let address = deps.api.addr_validate(user_addr.as_str())?;
        let acc_first_stake = self.acc_first_stake.may_load(deps.storage, address.clone())?.unwrap_or(0);

        let mut unlocked_stake = Uint128::zero();
        if acc_first_stake != 0 && calculated_cycle > acc_first_stake {
            let first_acc_stake_cycle = self.acc_stake_cycle.may_load(deps.storage,
                                    (address.clone(), U64Key::from(acc_first_stake)))?.unwrap_or(Uint128::zero());

            unlocked_stake += first_acc_stake_cycle;
            let acc_second_stake = self.acc_second_stake.may_load(deps.storage, address.clone())?.unwrap_or(0);
            if acc_second_stake != 0 && calculated_cycle > acc_second_stake {
                let second_acc_stake_cycle = self.acc_stake_cycle.may_load(deps.storage,
                    (address.clone(), U64Key::from(acc_second_stake)))?.unwrap_or(Uint128::zero());

                unlocked_stake += second_acc_stake_cycle;
            }
        }
        let acc_withdrawable_stake = self.acc_withdrawable_stake.may_load(deps.storage, address.clone())?.unwrap_or(Uint128::zero());

        Ok(GetWithdrawableStakeResponse { 
            amount: acc_withdrawable_stake + unlocked_stake,
        })
    }

    pub fn query_unclaimed_rewards(&self, deps: Deps, env: Env, user_addr: String) -> StdResult<GetUnclaimedRewardsResponse> {
        let config = CONFIG.load(deps.storage)?;
    
        let elapsed_time = env.block.time.seconds()
            .checked_sub(config.initial_timestamp)
            .ok_or_else(|| StdError::generic_err("Invalid elapsed time"))?;
    
        let calculated_cycle = elapsed_time / config.period_duration;

        let user_info = self.query_user_info(deps, user_addr, calculated_cycle)?;
        let mut current_reward = user_info.acc_rewards - user_info.acc_withdrawable_stake;
        if calculated_cycle > user_info.last_active_cycle && user_info.acc_cycle_batches_burned != 0 {
            let reward_per_cycle = self.reward_per_cycle.may_load(deps.storage, U64Key::from(user_info.last_active_cycle))?.unwrap_or(Uint128::zero());
            let cycle_total_batches_burned = self.cycle_total_batches_burned.may_load(deps.storage, U64Key::from(user_info.last_active_cycle))?.unwrap_or(0);
            let last_cycle_acc_reward = (Uint128::from(user_info.acc_cycle_batches_burned) * reward_per_cycle) / Uint128::from(cycle_total_batches_burned);

            current_reward += last_cycle_acc_reward;
        }

        Ok(GetUnclaimedRewardsResponse { 
            amount: current_reward,
        })
    }
    
    pub fn query_current_cycle_rewards(&self, deps: Deps) -> StdResult<GetCurrentCycleRewards> {
        let base_state = self.base_state.load(deps.storage)?;

        Ok(GetCurrentCycleRewards{
            amount: (base_state.last_cycle_reward * Uint128::from(10000u128)) / Uint128::from(10020u128),
        })
    }

    pub fn query_unclaimed_fees(&self, deps: Deps, env: Env, user_addr: String) -> StdResult<GetUnclaimedFees> {
        let config = CONFIG.load(deps.storage)?;
    
        let elapsed_time = env.block.time.seconds()
            .checked_sub(config.initial_timestamp)
            .ok_or_else(|| StdError::generic_err("Invalid elapsed time"))?;
    
        let calculated_cycle = elapsed_time / config.period_duration;

        let base_state = self.base_state.load(deps.storage)?;
        
        let mut previous_started_cycle_temp = base_state.previous_started_cycle;
        let mut last_started_cycle_temp = base_state.last_started_cycle;
        if calculated_cycle != base_state.current_started_cycle {
            previous_started_cycle_temp = last_started_cycle_temp + 1;
            last_started_cycle_temp = base_state.current_started_cycle;
        }
        
        let cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(deps.storage, U64Key::from(last_started_cycle_temp + 1))?.unwrap_or(Uint128::zero());   
        let cycle_info = self.query_cycle_info(deps.clone(), last_started_cycle_temp)?;     
        let mut current_cycle_fees_per_stake_summed = Uint128::zero();
        if calculated_cycle > last_started_cycle_temp && cycle_fees_per_stake_summed == Uint128::zero() {
            let mut fee_per_stake = Uint128::zero();
            if cycle_info.summed_cycle_stakes != Uint128::zero() {
                fee_per_stake = (cycle_info.cycle_accrued_fees + base_state.pending_fees) * Uint128::from(SCALING_FACTOR) / cycle_info.summed_cycle_stakes;
            }
            let previous_cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(deps.storage, U64Key::from(previous_started_cycle_temp))?.unwrap_or(Uint128::zero());
            current_cycle_fees_per_stake_summed = previous_cycle_fees_per_stake_summed + fee_per_stake;
        } else {
            current_cycle_fees_per_stake_summed = cycle_info.cycle_fees_per_stake_summed;
        }

        let user_info = self.query_user_info(deps.clone(), user_addr.clone(), calculated_cycle)?;
        let unclaimed_rewards = self.query_unclaimed_rewards(deps.clone(), env, user_addr.clone())?.amount;

        let current_rewards = unclaimed_rewards + user_info.acc_withdrawable_stake;
        let mut current_accrued_fees = user_info.acc_accrued_fees;
        if calculated_cycle > last_started_cycle_temp && user_info.last_fee_update_cycle != last_started_cycle_temp + 1 {
            let last_cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(deps.storage, U64Key::from(user_info.last_fee_update_cycle))?.unwrap_or(Uint128::zero());
            current_accrued_fees += (current_rewards * (current_cycle_fees_per_stake_summed - last_cycle_fees_per_stake_summed)) / Uint128::from(SCALING_FACTOR);
        }

        if user_info.acc_first_stake != 0 && calculated_cycle > user_info.acc_first_stake && last_started_cycle_temp + 1 > user_info.acc_first_stake {
            let address = deps.api.addr_validate(user_addr.as_str())?;
            let acc_stake_cycle = self.acc_stake_cycle.may_load(deps.storage,
                (address.clone(), 
                 U64Key::from(user_info.acc_first_stake)))?.unwrap_or(Uint128::zero());
            let first_cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(deps.storage, U64Key::from(user_info.acc_first_stake))?.unwrap_or(Uint128::zero());
            current_accrued_fees += (acc_stake_cycle * (current_cycle_fees_per_stake_summed - first_cycle_fees_per_stake_summed)) / Uint128::from(SCALING_FACTOR);
            
            if user_info.acc_second_stake != 0 && calculated_cycle > user_info.acc_second_stake && last_started_cycle_temp + 1 > user_info.acc_second_stake {
                let acc_stake_cycle = self.acc_stake_cycle.may_load(deps.storage,
                    (address.clone(), 
                     U64Key::from(user_info.acc_second_stake)))?.unwrap_or(Uint128::zero());
                let second_cycle_fees_per_stake_summed = self.cycle_fees_per_stake_summed.may_load(deps.storage, U64Key::from(user_info.acc_second_stake))?.unwrap_or(Uint128::zero());
                current_accrued_fees += (acc_stake_cycle * (current_cycle_fees_per_stake_summed - second_cycle_fees_per_stake_summed)) / Uint128::from(SCALING_FACTOR);
            }
        }
        Ok(GetUnclaimedFees {
            amount: current_accrued_fees,
        })
    }

}
