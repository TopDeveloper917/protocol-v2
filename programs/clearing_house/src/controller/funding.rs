use std::cmp::{max, min};

use anchor_lang::prelude::*;
use solana_program::clock::UnixTimestamp;
use solana_program::msg;

use crate::controller::amm::formulaic_update_k;
use crate::controller::position::{
    get_position_index, update_quote_asset_amount, PositionDirection,
};
use crate::error::ClearingHouseResult;
use crate::get_then_update_id;
use crate::math::amm;
use crate::math::casting::{cast, cast_to_i128};
use crate::math::constants::{FUNDING_RATE_BUFFER, ONE_HOUR, TWENTY_FOUR_HOUR};
use crate::math::funding::{calculate_funding_payment, calculate_funding_rate_long_short};
use crate::math::helpers::on_the_hour_update;
use crate::math::stats::calculate_new_twap;

use crate::math::oracle;
use crate::math_error;
use crate::state::events::{FundingPaymentRecord, FundingRateRecord};
use crate::state::market::{PerpMarket, AMM};
use crate::state::oracle_map::OracleMap;
use crate::state::perp_market_map::PerpMarketMap;
use crate::state::state::OracleGuardRails;
use crate::state::user::User;

pub fn settle_funding_payment(
    user: &mut User,
    user_key: &Pubkey,
    market: &mut PerpMarket,
    now: UnixTimestamp,
) -> ClearingHouseResult {
    let position_index = match get_position_index(&user.perp_positions, market.market_index) {
        Ok(position_index) => position_index,
        Err(_) => return Ok(()),
    };

    let mut market_position = &mut user.perp_positions[position_index];

    if market_position.base_asset_amount == 0 {
        return Ok(());
    }

    let amm: &AMM = &market.amm;

    let amm_cumulative_funding_rate = if market_position.base_asset_amount > 0 {
        amm.cumulative_funding_rate_long
    } else {
        amm.cumulative_funding_rate_short
    };

    if amm_cumulative_funding_rate != market_position.last_cumulative_funding_rate {
        let market_funding_payment =
            calculate_funding_payment(amm_cumulative_funding_rate, market_position)?;

        emit!(FundingPaymentRecord {
            ts: now,
            user_authority: user.authority,
            user: *user_key,
            market_index: market_position.market_index,
            funding_payment: market_funding_payment, //10e13
            user_last_cumulative_funding: market_position.last_cumulative_funding_rate, //10e14
            amm_cumulative_funding_long: amm.cumulative_funding_rate_long, //10e14
            amm_cumulative_funding_short: amm.cumulative_funding_rate_short, //10e14
            base_asset_amount: market_position.base_asset_amount, //10e13
        });

        market_position.last_cumulative_funding_rate = amm_cumulative_funding_rate;
        update_quote_asset_amount(market_position, market, market_funding_payment)?;
    }

    Ok(())
}

pub fn settle_funding_payments(
    user: &mut User,
    user_key: &Pubkey,
    perp_market_map: &PerpMarketMap,
    now: UnixTimestamp,
) -> ClearingHouseResult {
    for market_position in user.perp_positions.iter_mut() {
        if market_position.base_asset_amount == 0 {
            continue;
        }

        let market = &mut perp_market_map.get_ref_mut(&market_position.market_index)?;
        let amm: &AMM = &market.amm;

        let amm_cumulative_funding_rate = if market_position.base_asset_amount > 0 {
            amm.cumulative_funding_rate_long
        } else {
            amm.cumulative_funding_rate_short
        };

        if amm_cumulative_funding_rate != market_position.last_cumulative_funding_rate {
            let market_funding_payment =
                calculate_funding_payment(amm_cumulative_funding_rate, market_position)?;

            emit!(FundingPaymentRecord {
                ts: now,
                user_authority: user.authority,
                user: *user_key,
                market_index: market_position.market_index,
                funding_payment: market_funding_payment, //1e6
                user_last_cumulative_funding: market_position.last_cumulative_funding_rate, //1e9
                amm_cumulative_funding_long: amm.cumulative_funding_rate_long, //1e9
                amm_cumulative_funding_short: amm.cumulative_funding_rate_short, //1e9
                base_asset_amount: market_position.base_asset_amount, //1e9
            });

            market_position.last_cumulative_funding_rate = amm_cumulative_funding_rate;
            update_quote_asset_amount(market_position, market, market_funding_payment)?;
        }
    }

    Ok(())
}

#[allow(clippy::comparison_chain)]
pub fn update_funding_rate(
    market_index: u16,
    market: &mut PerpMarket,
    oracle_map: &mut OracleMap,
    now: UnixTimestamp,
    guard_rails: &OracleGuardRails,
    funding_paused: bool,
    precomputed_reserve_price: Option<u128>,
) -> ClearingHouseResult<bool> {
    let reserve_price = match precomputed_reserve_price {
        Some(reserve_price) => reserve_price,
        None => market.amm.reserve_price()?,
    };
    // Pause funding if oracle is invalid or if mark/oracle spread is too divergent
    let block_funding_rate_update = oracle::block_operation(
        market,
        oracle_map.get_price_data(&market.amm.oracle)?,
        guard_rails,
        Some(reserve_price),
    )?;

    let time_until_next_update = on_the_hour_update(
        now,
        market.amm.last_funding_rate_ts,
        market.amm.funding_period,
    )?;

    let valid_funding_update =
        !funding_paused && !block_funding_rate_update && (time_until_next_update == 0);

    if valid_funding_update {
        let oracle_price_data = oracle_map.get_price_data(&market.amm.oracle)?;
        let oracle_price_twap = amm::update_oracle_price_twap(
            &mut market.amm,
            now,
            oracle_price_data,
            Some(reserve_price),
        )?;

        // price relates to execution premium / direction
        let (execution_premium_price, execution_premium_direction) =
            if market.amm.long_spread > market.amm.short_spread {
                (
                    market.amm.ask_price(reserve_price)?,
                    Some(PositionDirection::Long),
                )
            } else if market.amm.long_spread < market.amm.short_spread {
                (
                    market.amm.bid_price(reserve_price)?,
                    Some(PositionDirection::Short),
                )
            } else {
                (reserve_price, None)
            };

        let mid_price_twap = amm::update_mark_twap(
            &mut market.amm,
            now,
            Some(execution_premium_price),
            execution_premium_direction,
        )?;

        let period_adjustment = (24_i128)
            .checked_mul(ONE_HOUR)
            .ok_or_else(math_error!())?
            .checked_div(max(ONE_HOUR, market.amm.funding_period as i128))
            .ok_or_else(math_error!())?;
        // funding period = 1 hour, window = 1 day
        // low periodicity => quickly updating/settled funding rates => lower funding rate payment per interval
        let price_spread = cast_to_i128(mid_price_twap)?
            .checked_sub(oracle_price_twap)
            .ok_or_else(math_error!())?;

        // clamp price divergence to 3% for funding rate calculation
        let max_price_spread = oracle_price_twap
            .checked_div(33)
            .ok_or_else(math_error!())?; // 3%
        let clamped_price_spread = max(-max_price_spread, min(price_spread, max_price_spread));

        let funding_rate = clamped_price_spread
            .checked_mul(cast(FUNDING_RATE_BUFFER)?)
            .ok_or_else(math_error!())?
            .checked_div(cast(period_adjustment)?)
            .ok_or_else(math_error!())?;

        let (funding_rate_long, funding_rate_short, funding_imbalance_cost) =
            calculate_funding_rate_long_short(market, funding_rate)?;

        // todo: finish robust tests
        if market.amm.curve_update_intensity > 0 {
            formulaic_update_k(market, oracle_price_data, funding_imbalance_cost, now)?;
        }

        market.amm.cumulative_funding_rate_long = market
            .amm
            .cumulative_funding_rate_long
            .checked_add(funding_rate_long)
            .ok_or_else(math_error!())?;

        market.amm.cumulative_funding_rate_short = market
            .amm
            .cumulative_funding_rate_short
            .checked_add(funding_rate_short)
            .ok_or_else(math_error!())?;

        market.amm.last_funding_rate = funding_rate;
        market.amm.last_funding_rate_long = funding_rate_long;
        market.amm.last_funding_rate_short = funding_rate_short;
        market.amm.last_24h_avg_funding_rate = calculate_new_twap(
            funding_rate,
            now,
            market.amm.last_24h_avg_funding_rate,
            market.amm.last_funding_rate_ts,
            TWENTY_FOUR_HOUR,
        )?;
        market.amm.last_funding_rate_ts = now;

        emit!(FundingRateRecord {
            ts: now,
            record_id: get_then_update_id!(market, next_funding_rate_record_id),
            market_index,
            funding_rate,
            funding_rate_long,
            funding_rate_short,
            cumulative_funding_rate_long: market.amm.cumulative_funding_rate_long,
            cumulative_funding_rate_short: market.amm.cumulative_funding_rate_short,
            mark_price_twap: mid_price_twap,
            oracle_price_twap,
            period_revenue: market.amm.net_revenue_since_last_funding,
            net_base_asset_amount: market.amm.net_base_asset_amount,
            net_unsettled_lp_base_asset_amount: market.amm.net_unsettled_lp_base_asset_amount,
        });

        market.amm.net_revenue_since_last_funding = 0;
    } else {
        return Ok(false);
    }

    Ok(true)
}
