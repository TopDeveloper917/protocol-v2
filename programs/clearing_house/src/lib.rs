use anchor_lang::prelude::*;
use borsh::{BorshDeserialize, BorshSerialize};

use controller::position::PositionDirection;
use error::*;
use instructions::*;
use math::{amm, bn, constants::*, fees, margin::*, position::*, withdrawal::*};
use state::{
    history::trade::TradeRecord,
    market::{Market, Markets, OracleSource, AMM},
    state::State,
    user::{MarketPosition, User},
};

mod controller;
mod error;
mod instructions;
mod math;
mod state;
declare_id!("9vNbzHGb1WstrTr2x2Etm7rqQLAM6BJA5VS9Mzto2Efw");

#[program]
pub mod clearing_house {
    use super::*;
    use crate::state::history::liquidation::LiquidationRecord;

    pub fn initialize(
        ctx: Context<Initialize>,
        _clearing_house_nonce: u8,
        _collateral_vault_nonce: u8,
        _insurance_vault_nonce: u8,
        admin_controls_prices: bool,
    ) -> ProgramResult {
        let collateral_account_key = ctx.accounts.collateral_vault.to_account_info().key;
        let (collateral_account_authority, collateral_account_nonce) =
            Pubkey::find_program_address(&[collateral_account_key.as_ref()], ctx.program_id);

        if ctx.accounts.collateral_vault.owner != collateral_account_authority {
            return Err(ErrorCode::InvalidCollateralAccountAuthority.into());
        }

        let insurance_account_key = ctx.accounts.insurance_vault.to_account_info().key;
        let (insurance_account_authority, insurance_account_nonce) =
            Pubkey::find_program_address(&[insurance_account_key.as_ref()], ctx.program_id);

        if ctx.accounts.insurance_vault.owner != insurance_account_authority {
            return Err(ErrorCode::InvalidInsuranceAccountAuthority.into());
        }

        ctx.accounts.markets.load_init()?;
        ctx.accounts.funding_payment_history.load_init()?;
        ctx.accounts.trade_history.load_init()?;

        **ctx.accounts.state = State {
            admin: *ctx.accounts.admin.key,
            admin_controls_prices,
            collateral_mint: *ctx.accounts.collateral_mint.to_account_info().key,
            collateral_vault: *collateral_account_key,
            collateral_vault_authority: collateral_account_authority,
            collateral_vault_nonce: collateral_account_nonce,
            trade_history: *ctx.accounts.trade_history.to_account_info().key,
            funding_payment_history: *ctx.accounts.funding_payment_history.to_account_info().key,
            liquidation_history: *ctx.accounts.liquidation_history.to_account_info().key,
            insurance_vault: *insurance_account_key,
            insurance_vault_authority: insurance_account_authority,
            insurance_vault_nonce: insurance_account_nonce,
            markets: *ctx.accounts.markets.to_account_info().key,
            margin_ratio_initial: 950, // unit is 9.5% (+2 decimal places)
            margin_ratio_partial: 625,
            margin_ratio_maintenance: 500,
            partial_liquidation_close_percentage_numerator: 25,
            partial_liquidation_close_percentage_denominator: 100,
            partial_liquidation_penalty_percentage_numerator: 25,
            partial_liquidation_penalty_percentage_denominator: 1000,
            full_liquidation_penalty_percentage_numerator: 1,
            full_liquidation_penalty_percentage_denominator: 1,
            partial_liquidation_liquidator_share_denominator: 2,
            full_liquidation_liquidator_share_denominator: 20,
            fee_numerator: DEFAULT_FEE_NUMERATOR,
            fee_denominator: DEFAULT_FEE_DENOMINATOR,
            collateral_deposits: 0,
            fees_collected: 0,
            fees_withdrawn: 0,
        };

        return Ok(());
    }

    pub fn initialize_market(
        ctx: Context<InitializeMarket>,
        market_index: u64,
        amm_base_asset_amount: u128,
        amm_quote_asset_amount: u128,
        amm_periodicity: i64,
        amm_peg_multiplier: u128,
    ) -> ProgramResult {
        let markets = &mut ctx.accounts.markets.load_mut()?;
        let market = &markets.markets[Markets::index_from_u64(market_index)];
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        if market.initialized {
            return Err(ErrorCode::MarketIndexAlreadyInitialized.into());
        }

        if amm_base_asset_amount != amm_quote_asset_amount {
            return Err(ErrorCode::InvalidInitialPeg.into());
        }

        let init_mark_price = amm::calculate_price(
            amm_quote_asset_amount,
            amm_base_asset_amount,
            amm_peg_multiplier,
        )?;

        // Verify there's no overflow
        let _k = bn::U256::from(amm_base_asset_amount)
            .checked_mul(bn::U256::from(amm_quote_asset_amount))
            .ok_or_else(math_error!())?;

        let market = Market {
            initialized: true,
            base_asset_amount_long: 0,
            base_asset_amount_short: 0,
            base_asset_amount: 0,
            open_interest: 0,
            amm: AMM {
                oracle: *ctx.accounts.oracle.key,
                oracle_source: OracleSource::Pyth,
                base_asset_reserve: amm_base_asset_amount,
                quote_asset_reserve: amm_quote_asset_amount,
                cumulative_funding_rate: 0,
                cumulative_repeg_rebate_long: 0,
                cumulative_repeg_rebate_short: 0,
                cumulative_funding_rate_long: 0,
                cumulative_funding_rate_short: 0,
                last_funding_rate: 0,
                last_funding_rate_ts: now,
                funding_period: amm_periodicity,
                last_mark_price_twap: init_mark_price,
                last_mark_price_twap_ts: now,
                sqrt_k: amm_base_asset_amount,
                peg_multiplier: amm_peg_multiplier,
                cumulative_fee: 0,
                cumulative_fee_realized: 0,
            },
        };

        markets.markets[Markets::index_from_u64(market_index)] = market;

        Ok(())
    }

    pub fn deposit_collateral(ctx: Context<DepositCollateral>, amount: u64) -> ProgramResult {
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        if amount == 0 {
            return Err(ErrorCode::InsufficientDeposit.into());
        }

        let user = &mut ctx.accounts.user;
        user.collateral = user
            .collateral
            .checked_add(amount as u128)
            .ok_or_else(math_error!())?;
        user.cumulative_deposits = user
            .cumulative_deposits
            .checked_add(amount as i128)
            .ok_or_else(math_error!())?;

        let markets = &ctx.accounts.markets.load()?;
        let user_positions = &mut ctx.accounts.user_positions.load_mut()?;
        let funding_payment_history = &mut ctx.accounts.funding_payment_history.load_mut()?;
        controller::funding::settle_funding_payment(
            user,
            user_positions,
            markets,
            funding_payment_history,
            now,
        )?;

        controller::token::receive(
            &ctx.accounts.token_program,
            &ctx.accounts.user_collateral_account,
            &ctx.accounts.collateral_vault,
            &ctx.accounts.authority,
            amount,
        )?;

        ctx.accounts.state.collateral_deposits = ctx
            .accounts
            .state
            .collateral_deposits
            .checked_add(amount as u128)
            .ok_or_else(math_error!())?;

        Ok(())
    }

    pub fn withdraw_collateral(ctx: Context<WithdrawCollateral>, amount: u64) -> ProgramResult {
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        let user = &mut ctx.accounts.user;

        let markets = &ctx.accounts.markets.load()?;
        let user_positions = &mut ctx.accounts.user_positions.load_mut()?;
        let funding_payment_history = &mut ctx.accounts.funding_payment_history.load_mut()?;
        controller::funding::settle_funding_payment(
            user,
            user_positions,
            markets,
            funding_payment_history,
            now,
        )?;

        if (amount as u128) > user.collateral {
            return Err(ErrorCode::InsufficientCollateral.into());
        }

        user.cumulative_deposits = user
            .cumulative_deposits
            .checked_sub(amount as i128)
            .ok_or_else(math_error!())?;

        let (collateral_account_withdrawal, insurance_account_withdrawal) =
            calculate_withdrawal_amounts(
                amount,
                &ctx.accounts.collateral_vault,
                &ctx.accounts.insurance_vault,
            )?;

        user.collateral = user
            .collateral
            .checked_sub(collateral_account_withdrawal as u128)
            .ok_or_else(math_error!())?
            .checked_sub(insurance_account_withdrawal as u128)
            .ok_or_else(math_error!())?;

        let (_total_collateral, _unrealized_pnl, _base_asset_value, margin_ratio) =
            calculate_margin_ratio(user, user_positions, markets)?;
        if margin_ratio < ctx.accounts.state.margin_ratio_initial {
            return Err(ErrorCode::InsufficientCollateral.into());
        }

        controller::token::send(
            &ctx.accounts.token_program,
            &ctx.accounts.collateral_vault,
            &ctx.accounts.user_collateral_account,
            &ctx.accounts.collateral_vault_authority,
            ctx.accounts.state.collateral_vault_nonce,
            collateral_account_withdrawal,
        )?;

        ctx.accounts.state.collateral_deposits = ctx
            .accounts
            .state
            .collateral_deposits
            .checked_sub(collateral_account_withdrawal as u128)
            .ok_or_else(math_error!())?;

        if insurance_account_withdrawal > 0 {
            controller::token::send(
                &ctx.accounts.token_program,
                &ctx.accounts.insurance_vault,
                &ctx.accounts.user_collateral_account,
                &ctx.accounts.insurance_vault_authority,
                ctx.accounts.state.insurance_vault_nonce,
                insurance_account_withdrawal,
            )?;
        }
        Ok(())
    }

    #[access_control(
        market_initialized(&ctx.accounts.markets, market_index)
    )]
    pub fn open_position<'info>(
        ctx: Context<OpenPosition>,
        direction: PositionDirection,
        quote_asset_amount: u128,
        market_index: u64,
        limit_price: u128,
    ) -> ProgramResult {
        let user = &mut ctx.accounts.user;
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        let user_positions = &mut ctx.accounts.user_positions.load_mut()?;
        let funding_payment_history = &mut ctx.accounts.funding_payment_history.load_mut()?;
        controller::funding::settle_funding_payment(
            user,
            user_positions,
            &ctx.accounts.markets.load()?,
            funding_payment_history,
            now,
        )?;

        let mut market_position = user_positions
            .positions
            .iter_mut()
            .find(|market_position| market_position.market_index == market_index);

        if market_position.is_none() {
            let available_position_index = user_positions
                .positions
                .iter()
                .position(|market_position| market_position.base_asset_amount == 0);

            if available_position_index.is_none() {
                return Err(ErrorCode::MaxNumberOfPositions.into());
            }

            let new_market_position = MarketPosition {
                market_index,
                base_asset_amount: 0,
                quote_asset_amount: 0,
                last_cumulative_funding_rate: 0,
                last_cumulative_repeg_rebate: 0,
                last_funding_rate_ts: 0,
            };

            user_positions.positions[available_position_index.unwrap()] = new_market_position;

            market_position =
                Some(&mut user_positions.positions[available_position_index.unwrap()]);
        }

        let market_position = market_position.unwrap();
        let base_asset_amount_before = market_position.base_asset_amount;
        let mark_price_before: u128;
        {
            let market = &mut ctx.accounts.markets.load_mut()?.markets
                [Markets::index_from_u64(market_index)];
            mark_price_before = market.amm.mark_price()?;
        }
        let mut potentially_risk_increasing = true;
        let mut is_oracle_mark_limit = false;

        if market_position.base_asset_amount == 0
            || market_position.base_asset_amount > 0 && direction == PositionDirection::Long
            || market_position.base_asset_amount < 0 && direction == PositionDirection::Short
        {
            let market = &mut ctx.accounts.markets.load_mut()?.markets
                [Markets::index_from_u64(market_index)];

            controller::position::increase(
                direction,
                quote_asset_amount,
                market,
                market_position,
                now,
            )?;

            let price_oracle = &ctx.accounts.oracle;
            is_oracle_mark_limit = amm::is_oracle_mark_limit(&market.amm, price_oracle, 0).unwrap();
        } else {
            let market = &mut ctx.accounts.markets.load_mut()?.markets
                [Markets::index_from_u64(market_index)];

            let (base_asset_value, _unrealized_pnl) =
                calculate_base_asset_value_and_pnl(market_position, &market.amm)?;
            // we calculate what the user's position is worth if they closed to determine
            // if they are reducing or closing and reversing their position
            if base_asset_value > quote_asset_amount {
                controller::position::reduce(
                    direction,
                    quote_asset_amount,
                    user,
                    market,
                    market_position,
                    now,
                )?;

                potentially_risk_increasing = false;
            } else {
                let incremental_quote_asset_notional_amount_resid = quote_asset_amount
                    .checked_sub(base_asset_value)
                    .ok_or_else(math_error!())?;

                if incremental_quote_asset_notional_amount_resid < base_asset_value {
                    potentially_risk_increasing = false; //todo
                }

                controller::position::close(user, market, market_position, now)?;

                controller::position::increase(
                    direction,
                    incremental_quote_asset_notional_amount_resid,
                    market,
                    market_position,
                    now,
                )?;
            }
        }

        let base_asset_amount_change = market_position
            .base_asset_amount
            .checked_sub(base_asset_amount_before)
            .ok_or_else(math_error!())?
            .unsigned_abs();
        let mark_price_after: u128;
        {
            let market = &mut ctx.accounts.markets.load_mut()?.markets
                [Markets::index_from_u64(market_index)];
            mark_price_after = market.amm.mark_price()?;
        }

        let fee = fees::calculate(
            quote_asset_amount,
            ctx.accounts.state.fee_numerator,
            ctx.accounts.state.fee_denominator,
        )?;
        ctx.accounts.state.fees_collected = ctx
            .accounts
            .state
            .fees_collected
            .checked_add(fee)
            .ok_or_else(math_error!())?;
        {
            let market = &mut ctx.accounts.markets.load_mut()?.markets
                [Markets::index_from_u64(market_index)];
            market.amm.cumulative_fee = market
                .amm
                .cumulative_fee
                .checked_add(fee)
                .ok_or_else(math_error!())?;
            market.amm.cumulative_fee_realized = market
                .amm
                .cumulative_fee_realized
                .checked_add(fee)
                .ok_or_else(math_error!())?;
        }

        user.collateral = user.collateral.checked_sub(fee).ok_or_else(math_error!())?;

        user.total_fee_paid = user
            .total_fee_paid
            .checked_add(fee)
            .ok_or_else(math_error!())?;

        let (
            _total_collateral_after,
            _unrealized_pnl_after,
            _base_asset_value_after,
            margin_ratio_after,
        ) = calculate_margin_ratio(user, user_positions, &ctx.accounts.markets.load()?)?;

        if margin_ratio_after < ctx.accounts.state.margin_ratio_initial
            && potentially_risk_increasing
        {
            return Err(ErrorCode::InsufficientCollateral.into());
        }

        if is_oracle_mark_limit && potentially_risk_increasing {
            return Err(ErrorCode::OracleMarkSpreadLimit.into());
        }

        let trade_history_account = &mut ctx.accounts.trade_history.load_mut()?;
        let record_id = trade_history_account.next_record_id();
        trade_history_account.append(TradeRecord {
            ts: now,
            record_id,
            user_authority: *ctx.accounts.authority.to_account_info().key,
            user: *user.to_account_info().key,
            direction,
            base_asset_amount: base_asset_amount_change,
            quote_asset_amount,
            mark_price_before,
            mark_price_after,
            fee,
            liquidation: false,
            market_index,
        });

        if limit_price != 0 {
            let market =
                &ctx.accounts.markets.load()?.markets[Markets::index_from_u64(market_index)];

            let scaled_quote_asset_amount =
                math::quote_asset::scale_to_amm_precision(quote_asset_amount)?;
            let unpegged_scaled_quote_asset_amount = math::quote_asset::unpeg_quote_asset_amount(
                scaled_quote_asset_amount,
                market.amm.peg_multiplier,
            )?;

            let entry_price = amm::calculate_price(
                unpegged_scaled_quote_asset_amount,
                base_asset_amount_change,
                market.amm.peg_multiplier,
            )?;

            match direction {
                PositionDirection::Long => {
                    if entry_price > limit_price {
                        return Err(ErrorCode::SlippageOutsideLimit.into());
                    }
                }
                PositionDirection::Short => {
                    if entry_price < limit_price {
                        return Err(ErrorCode::SlippageOutsideLimit.into());
                    }
                }
            }
        }

        {
            let market = &mut ctx.accounts.markets.load_mut()?.markets
                [Markets::index_from_u64(market_index)];
            let price_oracle = &ctx.accounts.oracle;
            controller::funding::update_funding_rate(market, &price_oracle, now)?;
        }

        Ok(())
    }

    #[access_control(
        market_initialized(&ctx.accounts.markets, market_index)
    )]
    pub fn close_position(ctx: Context<ClosePosition>, market_index: u64) -> ProgramResult {
        let user = &mut ctx.accounts.user;
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        let user_positions = &mut ctx.accounts.user_positions.load_mut()?;
        let funding_payment_history = &mut ctx.accounts.funding_payment_history.load_mut()?;
        controller::funding::settle_funding_payment(
            user,
            user_positions,
            &ctx.accounts.markets.load()?,
            funding_payment_history,
            now,
        )?;

        let market_position = user_positions
            .positions
            .iter_mut()
            .find(|market_position| market_position.market_index == market_index);

        if market_position.is_none() {
            return Err(ErrorCode::UserHasNoPositionInMarket.into());
        }
        let market_position = market_position.unwrap();

        let market =
            &mut ctx.accounts.markets.load_mut()?.markets[Markets::index_from_u64(market_index)];

        // base_asset_value is the base_asset_amount priced in quote_asset, so we can use this
        // as quote_asset_notional_amount in trade history
        let (base_asset_value, _pnl) =
            calculate_base_asset_value_and_pnl(market_position, &market.amm)?;
        let trade_history_account = &mut ctx.accounts.trade_history.load_mut()?;
        let record_id = trade_history_account.next_record_id();
        let mark_price_before = market.amm.mark_price()?;
        let direction_to_close =
            math::position::direction_to_close_position(market_position.base_asset_amount);
        let base_asset_amount = market_position.base_asset_amount.unsigned_abs();
        controller::position::close(user, market, market_position, now)?;

        let fee = fees::calculate(
            base_asset_value,
            ctx.accounts.state.fee_numerator,
            ctx.accounts.state.fee_denominator,
        )?;
        ctx.accounts.state.fees_collected = ctx
            .accounts
            .state
            .fees_collected
            .checked_add(fee)
            .ok_or_else(math_error!())?;
        market.amm.cumulative_fee = market
            .amm
            .cumulative_fee
            .checked_add(fee)
            .ok_or_else(math_error!())?;
        market.amm.cumulative_fee_realized = market
            .amm
            .cumulative_fee_realized
            .checked_add(fee)
            .ok_or_else(math_error!())?;

        user.collateral = user.collateral.checked_sub(fee).ok_or_else(math_error!())?;

        user.total_fee_paid = user
            .total_fee_paid
            .checked_add(fee)
            .ok_or_else(math_error!())?;

        let mark_price_after = market.amm.mark_price()?;
        trade_history_account.append(TradeRecord {
            ts: now,
            record_id,
            user_authority: *ctx.accounts.authority.to_account_info().key,
            user: *user.to_account_info().key,
            direction: direction_to_close,
            base_asset_amount,
            quote_asset_amount: base_asset_value,
            mark_price_before,
            mark_price_after,
            liquidation: false,
            fee,
            market_index,
        });

        let price_oracle = &ctx.accounts.oracle;
        controller::funding::update_funding_rate(market, &price_oracle, now)?;

        Ok(())
    }

    pub fn liquidate(ctx: Context<Liquidate>) -> ProgramResult {
        let state = &ctx.accounts.state;
        let user = &mut ctx.accounts.user;
        let trade_history = &mut ctx.accounts.trade_history.load_mut()?;
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        let collateral = user.collateral;
        let (total_collateral, unrealized_pnl, base_asset_value, margin_ratio) =
            calculate_margin_ratio(
                user,
                &ctx.accounts.user_positions.load_mut()?,
                &ctx.accounts.markets.load()?,
            )?;
        if margin_ratio > ctx.accounts.state.margin_ratio_partial {
            return Err(ErrorCode::SufficientCollateral.into());
        }

        let user_positions = &mut ctx.accounts.user_positions.load_mut()?;

        let mut is_full_liquidation = true;
        let mut base_asset_value_closed: u128 = 0;
        if margin_ratio <= ctx.accounts.state.margin_ratio_maintenance {
            let markets = &mut ctx.accounts.markets.load_mut()?;
            for market_position in user_positions.positions.iter_mut() {
                if market_position.base_asset_amount == 0 {
                    continue;
                }

                let market =
                    &mut markets.markets[Markets::index_from_u64(market_position.market_index)];

                let direction_to_close =
                    math::position::direction_to_close_position(market_position.base_asset_amount);
                let (base_asset_value, _pnl) = math::position::calculate_base_asset_value_and_pnl(
                    &market_position,
                    &market.amm,
                )?;
                base_asset_value_closed += base_asset_value;
                let base_asset_amount = market_position.base_asset_amount.unsigned_abs();

                let mark_price_before = market.amm.mark_price()?;
                controller::position::close(user, market, market_position, now)?;
                let mark_price_after = market.amm.mark_price()?;

                let record_id = trade_history.next_record_id();
                trade_history.append(TradeRecord {
                    ts: now,
                    record_id,
                    user_authority: user.authority,
                    user: *user.to_account_info().key,
                    direction: direction_to_close,
                    base_asset_amount,
                    quote_asset_amount: base_asset_value,
                    mark_price_before,
                    mark_price_after,
                    fee: 0,
                    liquidation: true,
                    market_index: market_position.market_index,
                });
            }
        } else {
            let markets = &mut ctx.accounts.markets.load_mut()?;
            for market_position in user_positions.positions.iter_mut() {
                if market_position.base_asset_amount == 0 {
                    continue;
                }

                let market =
                    &mut markets.markets[Markets::index_from_u64(market_position.market_index)];

                let (base_asset_value, _pnl) =
                    calculate_base_asset_value_and_pnl(market_position, &market.amm)?;
                let base_asset_value_to_close = base_asset_value
                    .checked_mul(state.partial_liquidation_close_percentage_numerator.into())
                    .ok_or_else(math_error!())?
                    .checked_div(
                        state
                            .partial_liquidation_close_percentage_denominator
                            .into(),
                    )
                    .ok_or_else(math_error!())?;
                base_asset_value_closed += base_asset_value_to_close;

                let direction_to_reduce =
                    math::position::direction_to_close_position(market_position.base_asset_amount);
                let mark_price_before = market.amm.mark_price()?;
                let base_asset_amount_before = market_position.base_asset_amount;

                controller::position::reduce(
                    direction_to_reduce,
                    base_asset_value_to_close,
                    user,
                    market,
                    market_position,
                    now,
                )?;

                let base_asset_amount_change = market_position
                    .base_asset_amount
                    .checked_sub(base_asset_amount_before)
                    .ok_or_else(math_error!())?
                    .unsigned_abs();

                let mark_price_after = market.amm.mark_price()?;
                let record_id = trade_history.next_record_id();
                trade_history.append(TradeRecord {
                    ts: now,
                    record_id,
                    user_authority: user.authority,
                    user: *user.to_account_info().key,
                    direction: direction_to_reduce,
                    base_asset_amount: base_asset_amount_change,
                    quote_asset_amount: base_asset_value_to_close,
                    mark_price_before,
                    mark_price_after,
                    fee: 0,
                    liquidation: true,
                    market_index: market_position.market_index,
                });
            }

            is_full_liquidation = false;
        }

        let liquidation_fee = if is_full_liquidation {
            user.collateral
                .checked_mul(state.full_liquidation_penalty_percentage_numerator.into())
                .ok_or_else(math_error!())?
                .checked_div(state.full_liquidation_penalty_percentage_denominator.into())
                .ok_or_else(math_error!())?
        } else {
            let markets = &ctx.accounts.markets.load()?;
            let (
                total_collateral_after,
                _unrealized_pnl_after,
                _base_asset_value_after,
                _margin_ratio_after,
            ) = calculate_margin_ratio(user, user_positions, markets)?;

            total_collateral_after
                .checked_mul(
                    state
                        .partial_liquidation_penalty_percentage_numerator
                        .into(),
                )
                .ok_or_else(math_error!())?
                .checked_div(
                    state
                        .partial_liquidation_penalty_percentage_denominator
                        .into(),
                )
                .ok_or_else(math_error!())?
        };

        let (withdrawal_amount, _) = calculate_withdrawal_amounts(
            liquidation_fee as u64,
            &ctx.accounts.collateral_vault,
            &ctx.accounts.insurance_vault,
        )?;

        user.collateral = user
            .collateral
            .checked_sub(liquidation_fee)
            .ok_or_else(math_error!())?;

        let fee_to_liquidator = if is_full_liquidation {
            withdrawal_amount
                .checked_div(state.full_liquidation_liquidator_share_denominator)
                .ok_or_else(math_error!())?
        } else {
            withdrawal_amount
                .checked_div(state.partial_liquidation_liquidator_share_denominator)
                .ok_or_else(math_error!())?
        };

        let fee_to_insurance_fund = withdrawal_amount
            .checked_sub(fee_to_liquidator)
            .ok_or_else(math_error!())?;

        if fee_to_liquidator > 0 {
            controller::token::send(
                &ctx.accounts.token_program,
                &ctx.accounts.collateral_vault,
                &ctx.accounts.liquidator_account,
                &ctx.accounts.collateral_vault_authority,
                ctx.accounts.state.collateral_vault_nonce,
                fee_to_liquidator,
            )?;
        }

        if fee_to_insurance_fund > 0 {
            controller::token::send(
                &ctx.accounts.token_program,
                &ctx.accounts.collateral_vault,
                &ctx.accounts.insurance_vault,
                &ctx.accounts.collateral_vault_authority,
                ctx.accounts.state.collateral_vault_nonce,
                fee_to_insurance_fund,
            )?;
        }

        let liquidation_history = &mut ctx.accounts.liquidation_history.load_mut()?;
        let record_id = liquidation_history.next_record_id();

        liquidation_history.append(LiquidationRecord {
            ts: now,
            record_id,
            user: user.to_account_info().key(),
            user_authority: user.authority,
            partial: !is_full_liquidation,
            base_asset_value,
            base_asset_value_closed,
            liquidation_fee,
            fee_to_liquidator,
            fee_to_insurance_fund,
            liquidator: ctx.accounts.liquidator.to_account_info().key(),
            total_collateral,
            collateral,
            unrealized_pnl,
            margin_ratio,
        });

        Ok(())
    }

    #[access_control(
        market_initialized(&ctx.accounts.markets, market_index)
    )]
    pub fn move_amm_price(
        ctx: Context<MoveAMMPrice>,
        base_asset_reserve: u128,
        quote_asset_reserve: u128,
        market_index: u64,
    ) -> ProgramResult {
        let markets = &mut ctx.accounts.markets.load_mut()?;
        let market = &mut markets.markets[Markets::index_from_u64(market_index)];
        controller::amm::move_price(&mut market.amm, base_asset_reserve, quote_asset_reserve)?;
        Ok(())
    }

    pub fn withdraw_fees(ctx: Context<WithdrawFees>, amount: u64) -> ProgramResult {
        let state = &mut ctx.accounts.state;

        let max_withdraw = state
            .fees_collected
            .checked_div(SHARE_OF_FEES_ALLOCATED_TO_REPEG_DENOMINATOR)
            .ok_or_else(math_error!())?
            .checked_sub(state.fees_withdrawn)
            .ok_or_else(math_error!())?;

        if (amount as u128) > max_withdraw {
            return Err(ErrorCode::AdminWithdrawTooLarge.into());
        }

        controller::token::send(
            &ctx.accounts.token_program,
            &ctx.accounts.collateral_vault,
            &ctx.accounts.recipient,
            &ctx.accounts.collateral_vault_authority,
            state.collateral_vault_nonce,
            amount,
        )?;

        state.fees_withdrawn = state
            .fees_withdrawn
            .checked_add(amount as u128)
            .ok_or_else(math_error!())?;

        Ok(())
    }

    pub fn withdraw_from_insurance_vault(
        ctx: Context<WithdrawFromInsuranceVault>,
        amount: u64,
    ) -> ProgramResult {
        controller::token::send(
            &ctx.accounts.token_program,
            &ctx.accounts.insurance_vault,
            &ctx.accounts.recipient,
            &ctx.accounts.insurance_vault_authority,
            ctx.accounts.state.insurance_vault_nonce,
            amount,
        )?;
        Ok(())
    }

    #[access_control(
        market_initialized(&ctx.accounts.markets, market_index)
    )]
    pub fn repeg_amm_curve(
        ctx: Context<RepegCurve>,
        new_peg_candidate: u128,
        market_index: u64,
    ) -> ProgramResult {
        let market =
            &mut ctx.accounts.markets.load_mut()?.markets[Markets::index_from_u64(market_index)];
        let price_oracle = &ctx.accounts.oracle;

        controller::repeg::repeg(market, price_oracle, new_peg_candidate)?;

        Ok(())
    }

    pub fn initialize_user(ctx: Context<InitializeUser>, _user_nonce: u8) -> ProgramResult {
        let user = &mut ctx.accounts.user;
        user.authority = *ctx.accounts.authority.key;
        user.collateral = 0;
        user.cumulative_deposits = 0;
        user.positions = *ctx.accounts.user_positions.to_account_info().key;

        let user_positions = &mut ctx.accounts.user_positions.load_init()?;
        user_positions.user = *ctx.accounts.user.to_account_info().key;

        Ok(())
    }

    pub fn settle_funding_payment(ctx: Context<SettleFunding>) -> ProgramResult {
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;
        controller::funding::settle_funding_payment(
            &mut ctx.accounts.user,
            &mut ctx.accounts.user_positions.load_mut()?,
            &ctx.accounts.markets.load()?,
            &mut ctx.accounts.funding_payment_history.load_mut()?,
            now,
        )?;
        Ok(())
    }

    #[access_control(
        market_initialized(&ctx.accounts.markets, market_index)
    )]
    pub fn update_funding_rate(
        ctx: Context<UpdateFundingRate>,
        market_index: u64,
    ) -> ProgramResult {
        let market =
            &mut ctx.accounts.markets.load_mut()?.markets[Markets::index_from_u64(market_index)];
        let price_oracle = &ctx.accounts.oracle;
        let clock = Clock::get()?;
        let now = clock.unix_timestamp;

        controller::funding::update_funding_rate(market, price_oracle, now)?;

        Ok(())
    }

    #[access_control(
        market_initialized(&ctx.accounts.markets, market_index)
    )]
    pub fn update_k(
        ctx: Context<AdminUpdateK>,
        base_asset_reserve: u128,
        quote_asset_reserve: u128,
        market_index: u64,
    ) -> ProgramResult {
        let markets = &mut ctx.accounts.markets.load_mut()?;
        let market = &mut markets.markets[Markets::index_from_u64(market_index)];
        let amm = &mut market.amm;

        let price_before = math::amm::calculate_price(
            amm.quote_asset_reserve,
            amm.base_asset_reserve,
            amm.peg_multiplier,
        )?;

        let price_after = math::amm::calculate_price(
            quote_asset_reserve,
            base_asset_reserve,
            amm.peg_multiplier,
        )?;

        let price_change_too_large = (price_before as i128)
            .checked_sub(price_after as i128)
            .ok_or_else(math_error!())?
            .unsigned_abs()
            .gt(&UPDATE_K_ALLOWED_PRICE_CHANGE);
        if price_change_too_large {
            return Err(ErrorCode::InvalidUpdateK.into());
        }

        controller::amm::move_price(amm, base_asset_reserve, quote_asset_reserve)?;
        Ok(())
    }

    pub fn update_margin_ratio(
        ctx: Context<AdminUpdateState>,
        margin_ratio_initial: u128,
        margin_ratio_partial: u128,
        margin_ratio_maintenance: u128,
    ) -> ProgramResult {
        ctx.accounts.state.margin_ratio_initial = margin_ratio_initial;
        ctx.accounts.state.margin_ratio_partial = margin_ratio_partial;
        ctx.accounts.state.margin_ratio_maintenance = margin_ratio_maintenance;
        Ok(())
    }

    pub fn update_partial_liquidation_close_percentage(
        ctx: Context<AdminUpdateState>,
        numerator: u128,
        denominator: u128,
    ) -> ProgramResult {
        ctx.accounts
            .state
            .partial_liquidation_close_percentage_numerator = numerator;
        ctx.accounts
            .state
            .partial_liquidation_close_percentage_denominator = denominator;
        Ok(())
    }

    pub fn update_partial_liquidation_penalty_percentage(
        ctx: Context<AdminUpdateState>,
        numerator: u128,
        denominator: u128,
    ) -> ProgramResult {
        ctx.accounts
            .state
            .partial_liquidation_penalty_percentage_numerator = numerator;
        ctx.accounts
            .state
            .partial_liquidation_penalty_percentage_denominator = denominator;
        Ok(())
    }

    pub fn update_full_liquidation_penalty_percentage(
        ctx: Context<AdminUpdateState>,
        numerator: u128,
        denominator: u128,
    ) -> ProgramResult {
        ctx.accounts
            .state
            .full_liquidation_penalty_percentage_numerator = numerator;
        ctx.accounts
            .state
            .full_liquidation_penalty_percentage_denominator = denominator;
        Ok(())
    }

    pub fn update_partial_liquidation_liquidator_share_denominator(
        ctx: Context<AdminUpdateState>,
        denominator: u64,
    ) -> ProgramResult {
        ctx.accounts
            .state
            .partial_liquidation_liquidator_share_denominator = denominator;
        Ok(())
    }

    pub fn update_full_liquidation_liquidator_share_denominator(
        ctx: Context<AdminUpdateState>,
        denominator: u64,
    ) -> ProgramResult {
        ctx.accounts
            .state
            .full_liquidation_liquidator_share_denominator = denominator;
        Ok(())
    }

    pub fn update_fee(
        ctx: Context<AdminUpdateState>,
        fee_numerator: u128,
        fee_denominator: u128,
    ) -> ProgramResult {
        ctx.accounts.state.fee_numerator = fee_numerator;
        ctx.accounts.state.fee_denominator = fee_denominator;
        Ok(())
    }

    pub fn update_admin(ctx: Context<AdminUpdateState>, admin: Pubkey) -> ProgramResult {
        ctx.accounts.state.admin = admin;
        Ok(())
    }
}

fn market_initialized(markets: &Loader<Markets>, market_index: u64) -> Result<()> {
    if !markets.load()?.markets[Markets::index_from_u64(market_index)].initialized {
        return Err(ErrorCode::MarketIndexNotInitialized.into());
    }
    Ok(())
}
