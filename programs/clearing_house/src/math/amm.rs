use crate::controller::amm::SwapDirection;
use crate::controller::position::PositionDirection;
use crate::error::{ClearingHouseResult, ErrorCode};
use crate::math::bn;
use crate::math::bn::U192;
use crate::math::casting::{cast, cast_to_i128, cast_to_u128, cast_to_u64};
use crate::math::constants::{
    AMM_RESERVE_PRECISION, AMM_RESERVE_PRECISION_I128, AMM_TIMES_PEG_TO_QUOTE_PRECISION_RATIO_I128,
    AMM_TO_QUOTE_PRECISION_RATIO_I128, BID_ASK_SPREAD_PRECISION, BID_ASK_SPREAD_PRECISION_I128,
    CONCENTRATION_PRECISION, DEFAULT_LARGE_BID_ASK_FACTOR, K_BPS_DECREASE_MAX, K_BPS_UPDATE_SCALE,
    MAX_BID_ASK_INVENTORY_SKEW_FACTOR, ONE_HOUR_I128, PEG_PRECISION, PRICE_PRECISION,
    PRICE_PRECISION_I128, PRICE_TO_PEG_PRECISION_RATIO, PRICE_TO_QUOTE_PRECISION_RATIO,
    QUOTE_PRECISION,
};
use crate::math::orders::standardize_base_asset_amount;
use crate::math::position::{_calculate_base_asset_value_and_pnl, calculate_base_asset_value};
use crate::math::quote_asset::reserve_to_asset_amount;
use crate::math::stats::{calculate_new_twap, calculate_weighted_average};
use crate::math_error;
use crate::state::market::{PerpMarket, AMM};
use crate::state::oracle::OraclePriceData;
use crate::state::state::PriceDivergenceGuardRails;
use crate::validate;
use solana_program::msg;
use std::cmp::{max, min};

use super::helpers::get_proportion_u128;

pub fn calculate_price(
    quote_asset_reserve: u128,
    base_asset_reserve: u128,
    peg_multiplier: u128,
) -> ClearingHouseResult<u128> {
    let peg_quote_asset_amount = quote_asset_reserve
        .checked_mul(peg_multiplier)
        .ok_or_else(math_error!())?;

    U192::from(peg_quote_asset_amount)
        .checked_mul(U192::from(PRICE_TO_PEG_PRECISION_RATIO))
        .ok_or_else(math_error!())?
        .checked_div(U192::from(base_asset_reserve))
        .ok_or_else(math_error!())?
        .try_to_u128()
}

pub fn calculate_bid_ask_bounds(
    concentration_coef: u128,
    sqrt_k: u128,
) -> ClearingHouseResult<(u128, u128)> {
    // worse case if all asks are filled (max reserve)
    let ask_bounded_base =
        get_proportion_u128(sqrt_k, concentration_coef, CONCENTRATION_PRECISION)?;

    // worse case if all bids are filled (min reserve)
    let bid_bounded_base =
        get_proportion_u128(sqrt_k, CONCENTRATION_PRECISION, concentration_coef)?;

    Ok((bid_bounded_base, ask_bounded_base))
}

pub fn calculate_terminal_price(amm: &mut AMM) -> ClearingHouseResult<u128> {
    let swap_direction = if amm.net_base_asset_amount > 0 {
        SwapDirection::Add
    } else {
        SwapDirection::Remove
    };
    let (new_quote_asset_amount, new_base_asset_amount) = calculate_swap_output(
        amm.net_base_asset_amount.unsigned_abs(),
        amm.base_asset_reserve,
        swap_direction,
        amm.sqrt_k,
    )?;

    let terminal_price = calculate_price(
        new_quote_asset_amount,
        new_base_asset_amount,
        amm.peg_multiplier,
    )?;

    Ok(terminal_price)
}

pub fn calculate_market_open_bids_asks(amm: &AMM) -> ClearingHouseResult<(i128, i128)> {
    let base_asset_reserve = amm.base_asset_reserve;
    let min_base_asset_reserve = amm.min_base_asset_reserve;
    let max_base_asset_reserve = amm.max_base_asset_reserve;

    let (max_bids, max_asks) = _calculate_market_open_bids_asks(
        base_asset_reserve,
        min_base_asset_reserve,
        max_base_asset_reserve,
    )?;

    Ok((max_bids, max_asks))
}

pub fn _calculate_market_open_bids_asks(
    base_asset_reserve: u128,
    min_base_asset_reserve: u128,
    max_base_asset_reserve: u128,
) -> ClearingHouseResult<(i128, i128)> {
    // worse case if all asks are filled
    let max_asks = if base_asset_reserve < max_base_asset_reserve {
        -cast_to_i128(
            max_base_asset_reserve
                .checked_sub(base_asset_reserve)
                .ok_or_else(math_error!())?,
        )?
    } else {
        0
    };

    // worst case if all bids are filled
    let max_bids = if base_asset_reserve > min_base_asset_reserve {
        cast_to_i128(
            base_asset_reserve
                .checked_sub(min_base_asset_reserve)
                .ok_or_else(math_error!())?,
        )?
    } else {
        0
    };

    Ok((max_bids, max_asks))
}

pub fn cap_to_max_spread(
    mut long_spread: u128,
    mut short_spread: u128,
    max_spread: u128,
) -> ClearingHouseResult<(u128, u128)> {
    let total_spread = long_spread
        .checked_add(short_spread)
        .ok_or_else(math_error!())?;

    if total_spread > max_spread {
        if long_spread > short_spread {
            long_spread = min(max_spread, long_spread);
            short_spread = max_spread
                .checked_sub(long_spread)
                .ok_or_else(math_error!())?;
        } else {
            short_spread = min(max_spread, short_spread);
            long_spread = max_spread
                .checked_sub(short_spread)
                .ok_or_else(math_error!())?;
        }
    }

    let new_total_spread = long_spread
        .checked_add(short_spread)
        .ok_or_else(math_error!())?;

    validate!(
        new_total_spread <= max_spread,
        ErrorCode::DefaultError,
        "new_total_spread({}) > max_spread({})",
        new_total_spread,
        max_spread
    )?;

    Ok((long_spread, short_spread))
}

#[allow(clippy::comparison_chain)]
pub fn calculate_spread(
    base_spread: u16,
    last_oracle_reserve_price_spread_pct: i128,
    last_oracle_conf_pct: u64,
    max_spread: u32,
    quote_asset_reserve: u128,
    terminal_quote_asset_reserve: u128,
    peg_multiplier: u128,
    net_base_asset_amount: i128,
    reserve_price: u128,
    total_fee_minus_distributions: i128,
    base_asset_reserve: u128,
    min_base_asset_reserve: u128,
    max_base_asset_reserve: u128,
) -> ClearingHouseResult<(u128, u128)> {
    let mut long_spread = (base_spread / 2) as u128;
    let mut short_spread = (base_spread / 2) as u128;

    // oracle retreat
    // if mark - oracle < 0 (mark below oracle) and user going long then increase spread
    if last_oracle_reserve_price_spread_pct < 0 {
        long_spread = max(
            long_spread,
            last_oracle_reserve_price_spread_pct
                .unsigned_abs()
                .checked_add(cast_to_u128(last_oracle_conf_pct)?)
                .ok_or_else(math_error!())?,
        );
    } else {
        short_spread = max(
            short_spread,
            last_oracle_reserve_price_spread_pct
                .unsigned_abs()
                .checked_add(cast_to_u128(last_oracle_conf_pct)?)
                .ok_or_else(math_error!())?,
        );
    }

    // inventory scale
    let (max_bids, max_asks) = _calculate_market_open_bids_asks(
        base_asset_reserve,
        min_base_asset_reserve,
        max_base_asset_reserve,
    )?;

    let min_side_liquidity = max_bids.min(max_asks.abs());

    // inventory scale
    let inventory_scale = net_base_asset_amount
        .checked_mul(cast_to_i128(DEFAULT_LARGE_BID_ASK_FACTOR)?)
        .ok_or_else(math_error!())?
        .checked_div(min_side_liquidity.max(1))
        .ok_or_else(math_error!())?
        .unsigned_abs();

    let inventory_scale_capped = min(
        MAX_BID_ASK_INVENTORY_SKEW_FACTOR,
        BID_ASK_SPREAD_PRECISION
            .checked_add(inventory_scale)
            .ok_or_else(math_error!())?,
    );

    if net_base_asset_amount > 0 {
        long_spread = long_spread
            .checked_mul(inventory_scale_capped)
            .ok_or_else(math_error!())?
            .checked_div(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?;
    } else if net_base_asset_amount < 0 {
        short_spread = short_spread
            .checked_mul(inventory_scale_capped)
            .ok_or_else(math_error!())?
            .checked_div(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?;
    }

    // effective leverage scale
    let net_base_asset_value = cast_to_i128(quote_asset_reserve)?
        .checked_sub(cast_to_i128(terminal_quote_asset_reserve)?)
        .ok_or_else(math_error!())?
        .checked_mul(cast_to_i128(peg_multiplier)?)
        .ok_or_else(math_error!())?
        .checked_div(AMM_TIMES_PEG_TO_QUOTE_PRECISION_RATIO_I128)
        .ok_or_else(math_error!())?;

    let local_base_asset_value = net_base_asset_amount
        .checked_mul(cast_to_i128(reserve_price)?)
        .ok_or_else(math_error!())?
        .checked_div(AMM_TO_QUOTE_PRECISION_RATIO_I128 * PRICE_PRECISION_I128)
        .ok_or_else(math_error!())?;

    let effective_leverage = max(
        0,
        local_base_asset_value
            .checked_sub(net_base_asset_value)
            .ok_or_else(math_error!())?,
    )
    .checked_mul(BID_ASK_SPREAD_PRECISION_I128)
    .ok_or_else(math_error!())?
    .checked_div(max(0, total_fee_minus_distributions) + 1)
    .ok_or_else(math_error!())?;

    let effective_leverage_capped = min(
        MAX_BID_ASK_INVENTORY_SKEW_FACTOR,
        BID_ASK_SPREAD_PRECISION
            .checked_add(cast_to_u128(max(0, effective_leverage))? + 1)
            .ok_or_else(math_error!())?,
    );

    if total_fee_minus_distributions <= 0 {
        long_spread = long_spread
            .checked_mul(DEFAULT_LARGE_BID_ASK_FACTOR)
            .ok_or_else(math_error!())?
            .checked_div(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?;
        short_spread = short_spread
            .checked_mul(DEFAULT_LARGE_BID_ASK_FACTOR)
            .ok_or_else(math_error!())?
            .checked_div(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?;
    } else if net_base_asset_amount > 0 {
        long_spread = long_spread
            .checked_mul(effective_leverage_capped)
            .ok_or_else(math_error!())?
            .checked_div(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?;
    } else {
        short_spread = short_spread
            .checked_mul(effective_leverage_capped)
            .ok_or_else(math_error!())?
            .checked_div(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?;
    }
    let (long_spread, short_spread) = cap_to_max_spread(
        long_spread,
        short_spread,
        cast_to_u128(max_spread)?.max(last_oracle_reserve_price_spread_pct.unsigned_abs()),
    )?;

    Ok((long_spread, short_spread))
}

pub fn update_mark_twap(
    amm: &mut AMM,
    now: i64,
    precomputed_trade_price: Option<u128>,
    direction: Option<PositionDirection>,
) -> ClearingHouseResult<u128> {
    let base_spread_u128 = cast_to_u128(amm.base_spread)?;
    let last_oracle_price_u128 = cast_to_u128(amm.historical_oracle_data.last_oracle_price)?;

    let trade_price: u128 = match precomputed_trade_price {
        Some(trade_price) => trade_price,
        None => last_oracle_price_u128,
    };

    validate!(
        amm.historical_oracle_data.last_oracle_price > 0,
        ErrorCode::InvalidOracle,
        "amm.historical_oracle_data.last_oracle_price <= 0"
    )?;

    let amm_reserve_price = amm.reserve_price()?;
    let (amm_bid_price, amm_ask_price) = amm.bid_ask_price(amm_reserve_price)?;
    // estimation of bid/ask by looking at execution premium

    // trade is a long
    let best_bid_estimate = if trade_price > last_oracle_price_u128 {
        let discount = min(base_spread_u128, amm.short_spread / 2);
        last_oracle_price_u128
            .checked_sub(discount)
            .ok_or_else(math_error!())?
    } else {
        trade_price
    };

    // trade is a short
    let best_ask_estimate = if trade_price < last_oracle_price_u128 {
        let premium = min(base_spread_u128, amm.long_spread / 2);
        last_oracle_price_u128
            .checked_add(premium)
            .ok_or_else(math_error!())?
    } else {
        trade_price
    };

    validate!(
        best_bid_estimate <= best_ask_estimate,
        ErrorCode::DefaultError,
        "best_bid_estimate({}, {}) not <= best_ask_estimate({}, {})",
        amm_bid_price,
        best_bid_estimate,
        best_ask_estimate,
        amm_ask_price,
    )?;

    let (bid_price, ask_price) = match direction {
        Some(direction) => match direction {
            PositionDirection::Long => (best_bid_estimate, trade_price),
            PositionDirection::Short => (trade_price, best_ask_estimate),
        },
        None => (trade_price, trade_price),
    };

    validate!(
        bid_price <= ask_price,
        ErrorCode::DefaultError,
        "bid_price({}, {}) not <= ask_price({}, {}),",
        best_bid_estimate,
        bid_price,
        ask_price,
        best_ask_estimate,
    )?;

    let (bid_price_capped_update, ask_price_capped_update) = (
        sanitize_new_price(
            cast_to_i128(bid_price)?,
            cast_to_i128(amm.last_bid_price_twap)?,
        )?,
        sanitize_new_price(
            cast_to_i128(ask_price)?,
            cast_to_i128(amm.last_ask_price_twap)?,
        )?,
    );

    validate!(
        bid_price_capped_update <= ask_price_capped_update,
        ErrorCode::DefaultError,
        "bid_price_capped_update not <= ask_price_capped_update,"
    )?;

    // update bid and ask twaps
    let bid_twap = calculate_new_twap(
        bid_price_capped_update,
        now,
        cast(amm.last_bid_price_twap)?,
        amm.last_mark_price_twap_ts,
        amm.funding_period,
    )?;
    amm.last_bid_price_twap = cast(bid_twap)?;

    let ask_twap = calculate_new_twap(
        ask_price_capped_update,
        now,
        cast(amm.last_ask_price_twap)?,
        amm.last_mark_price_twap_ts,
        amm.funding_period,
    )?;

    amm.last_ask_price_twap = cast(ask_twap)?;

    let mid_twap = bid_twap.checked_add(ask_twap).ok_or_else(math_error!())? / 2;

    // update std stat
    update_amm_mark_std(amm, now, trade_price, amm.last_mark_price_twap)?;

    amm.last_mark_price_twap = cast(mid_twap)?;
    amm.last_mark_price_twap_5min = cast(calculate_new_twap(
        cast(
            bid_price_capped_update
                .checked_add(ask_price_capped_update)
                .ok_or_else(math_error!())?
                / 2,
        )?,
        now,
        cast(amm.last_mark_price_twap_5min)?,
        amm.last_mark_price_twap_ts,
        60 * 5,
    )?)?;

    amm.last_mark_price_twap_ts = now;

    cast(mid_twap)
}

pub fn sanitize_new_price(new_price: i128, last_price_twap: i128) -> ClearingHouseResult<i128> {
    // when/if twap is 0, dont try to normalize new_price
    if last_price_twap == 0 {
        return Ok(new_price);
    }

    let new_price_spread = new_price
        .checked_sub(last_price_twap)
        .ok_or_else(math_error!())?;

    // cap new oracle update to 33% delta from twap
    let price_twap_33pct = last_price_twap.checked_div(3).ok_or_else(math_error!())?;

    let capped_update_price = if new_price_spread.unsigned_abs() > price_twap_33pct.unsigned_abs() {
        if new_price > last_price_twap {
            last_price_twap
                .checked_add(price_twap_33pct)
                .ok_or_else(math_error!())?
        } else {
            last_price_twap
                .checked_sub(price_twap_33pct)
                .ok_or_else(math_error!())?
        }
    } else {
        new_price
    };

    Ok(capped_update_price)
}

pub fn update_oracle_price_twap(
    amm: &mut AMM,
    now: i64,
    oracle_price_data: &OraclePriceData,
    precomputed_reserve_price: Option<u128>,
) -> ClearingHouseResult<i128> {
    let reserve_price = match precomputed_reserve_price {
        Some(reserve_price) => reserve_price,
        None => amm.reserve_price()?,
    };

    let oracle_price = normalise_oracle_price(amm, oracle_price_data, Some(reserve_price))?;

    let capped_oracle_update_price = sanitize_new_price(
        oracle_price,
        amm.historical_oracle_data.last_oracle_price_twap,
    )?;

    // sanity check
    let oracle_price_twap: i128;
    if capped_oracle_update_price > 0 && oracle_price > 0 {
        oracle_price_twap = calculate_new_oracle_price_twap(
            amm,
            now,
            capped_oracle_update_price,
            TwapPeriod::FundingPeriod,
        )?;

        let oracle_price_twap_5min = calculate_new_oracle_price_twap(
            amm,
            now,
            capped_oracle_update_price,
            TwapPeriod::FiveMin,
        )?;

        amm.last_oracle_normalised_price = capped_oracle_update_price;
        amm.historical_oracle_data.last_oracle_price = oracle_price_data.price;
        amm.last_oracle_conf_pct = oracle_price_data
            .confidence
            .checked_mul(BID_ASK_SPREAD_PRECISION)
            .ok_or_else(math_error!())?
            .checked_div(reserve_price)
            .ok_or_else(math_error!())? as u64;
        amm.historical_oracle_data.last_oracle_delay = oracle_price_data.delay;
        amm.last_oracle_reserve_price_spread_pct =
            calculate_oracle_reserve_price_spread_pct(amm, oracle_price_data, Some(reserve_price))?;

        amm.historical_oracle_data.last_oracle_price_twap_5min = oracle_price_twap_5min;
        amm.historical_oracle_data.last_oracle_price_twap = oracle_price_twap;
        amm.historical_oracle_data.last_oracle_price_twap_ts = now;
    } else {
        oracle_price_twap = amm.historical_oracle_data.last_oracle_price_twap
    }

    Ok(oracle_price_twap)
}

pub enum TwapPeriod {
    FundingPeriod,
    FiveMin,
}

pub fn calculate_new_oracle_price_twap(
    amm: &AMM,
    now: i64,
    oracle_price: i128,
    twap_period: TwapPeriod,
) -> ClearingHouseResult<i128> {
    let (last_mark_twap, last_oracle_twap) = match twap_period {
        TwapPeriod::FundingPeriod => (
            amm.last_mark_price_twap,
            amm.historical_oracle_data.last_oracle_price_twap,
        ),
        TwapPeriod::FiveMin => (
            amm.last_mark_price_twap_5min,
            amm.historical_oracle_data.last_oracle_price_twap_5min,
        ),
    };

    let period: i64 = match twap_period {
        TwapPeriod::FundingPeriod => amm.funding_period,
        TwapPeriod::FiveMin => 60 * 5,
    };

    let since_last = cast_to_i128(max(
        1,
        now.checked_sub(amm.historical_oracle_data.last_oracle_price_twap_ts)
            .ok_or_else(math_error!())?,
    ))?;
    let from_start = max(
        0,
        cast_to_i128(period)?
            .checked_sub(since_last)
            .ok_or_else(math_error!())?,
    );

    // if an oracle delay impacted last oracle_twap, shrink toward mark_twap
    let interpolated_oracle_price =
        if amm.last_mark_price_twap_ts > amm.historical_oracle_data.last_oracle_price_twap_ts {
            let since_last_valid = cast_to_i128(
                amm.last_mark_price_twap_ts
                    .checked_sub(amm.historical_oracle_data.last_oracle_price_twap_ts)
                    .ok_or_else(math_error!())?,
            )?;
            msg!(
                "correcting oracle twap update (oracle previously invalid for {:?} seconds)",
                since_last_valid
            );

            let from_start_valid = max(
                1,
                cast_to_i128(period)?
                    .checked_sub(since_last_valid)
                    .ok_or_else(math_error!())?,
            );
            calculate_weighted_average(
                cast_to_i128(last_mark_twap)?,
                oracle_price,
                since_last_valid,
                from_start_valid,
            )?
        } else {
            oracle_price
        };

    let new_twap = calculate_weighted_average(
        interpolated_oracle_price,
        last_oracle_twap,
        since_last,
        from_start,
    )?;

    Ok(new_twap)
}

pub fn update_amm_mark_std(
    amm: &mut AMM,
    now: i64,
    price: u128,
    ewma: u128,
) -> ClearingHouseResult<bool> {
    let since_last = cast_to_i128(max(
        1,
        now.checked_sub(amm.last_mark_price_twap_ts)
            .ok_or_else(math_error!())?,
    ))?;

    let price_change = cast_to_i128(price)?
        .checked_sub(cast_to_i128(ewma)?)
        .ok_or_else(math_error!())?;

    amm.mark_std = calculate_rolling_sum(
        amm.mark_std,
        cast_to_u64(price_change.unsigned_abs())?,
        max(ONE_HOUR_I128, since_last),
        ONE_HOUR_I128,
    )?;

    Ok(true)
}

pub fn update_amm_long_short_intensity(
    amm: &mut AMM,
    now: i64,
    quote_asset_amount: u128,
    direction: PositionDirection,
) -> ClearingHouseResult<bool> {
    let since_last = cast_to_i128(max(
        1,
        now.checked_sub(amm.last_trade_ts)
            .ok_or_else(math_error!())?,
    ))?;

    let (long_quote_amount, short_quote_amount) = if direction == PositionDirection::Long {
        (cast_to_u64(quote_asset_amount)?, 0_u64)
    } else {
        (0_u64, cast_to_u64(quote_asset_amount)?)
    };

    amm.long_intensity_count = (calculate_rolling_sum(
        cast_to_u64(amm.long_intensity_count)?,
        cast_to_u64(long_quote_amount != 0)?,
        since_last,
        ONE_HOUR_I128,
    )?) as u16;
    amm.long_intensity_volume = calculate_rolling_sum(
        amm.long_intensity_volume,
        long_quote_amount,
        since_last,
        ONE_HOUR_I128,
    )?;

    amm.short_intensity_count = (calculate_rolling_sum(
        cast_to_u64(amm.short_intensity_count)?,
        cast_to_u64(short_quote_amount != 0)?,
        since_last,
        ONE_HOUR_I128,
    )?) as u16;
    amm.short_intensity_volume = calculate_rolling_sum(
        amm.short_intensity_volume,
        short_quote_amount,
        since_last,
        ONE_HOUR_I128,
    )?;

    Ok(true)
}

pub fn calculate_rolling_sum(
    data1: u64,
    data2: u64,
    weight1_numer: i128,
    weight1_denom: i128,
) -> ClearingHouseResult<u64> {
    // assumes that missing times are zeros (e.g. handle NaN as 0)
    let prev_twap_99 = cast_to_u128(data1)?
        .checked_mul(cast_to_u128(max(
            0,
            weight1_denom
                .checked_sub(weight1_numer)
                .ok_or_else(math_error!())?,
        ))?)
        .ok_or_else(math_error!())?
        .checked_div(cast_to_u128(weight1_denom)?)
        .ok_or_else(math_error!())?;

    cast_to_u64(prev_twap_99)?
        .checked_add(data2)
        .ok_or_else(math_error!())
}

pub fn calculate_swap_output(
    swap_amount: u128,
    input_asset_reserve: u128,
    direction: SwapDirection,
    invariant_sqrt: u128,
) -> ClearingHouseResult<(u128, u128)> {
    let invariant_sqrt_u192 = U192::from(invariant_sqrt);
    let invariant = invariant_sqrt_u192
        .checked_mul(invariant_sqrt_u192)
        .ok_or_else(math_error!())?;

    if direction == SwapDirection::Remove && swap_amount > input_asset_reserve {
        msg!("{:?} > {:?}", swap_amount, input_asset_reserve);
        return Err(ErrorCode::TradeSizeTooLarge);
    }

    let new_input_asset_reserve = if let SwapDirection::Add = direction {
        input_asset_reserve
            .checked_add(swap_amount)
            .ok_or_else(math_error!())?
    } else {
        input_asset_reserve
            .checked_sub(swap_amount)
            .ok_or_else(math_error!())?
    };

    let new_input_amount_u192 = U192::from(new_input_asset_reserve);
    let new_output_asset_reserve = invariant
        .checked_div(new_input_amount_u192)
        .ok_or_else(math_error!())?
        .try_to_u128()?;

    Ok((new_output_asset_reserve, new_input_asset_reserve))
}

pub fn calculate_quote_asset_amount_swapped(
    quote_asset_reserve_before: u128,
    quote_asset_reserve_after: u128,
    swap_direction: SwapDirection,
    peg_multiplier: u128,
) -> ClearingHouseResult<u128> {
    let quote_asset_reserve_change = match swap_direction {
        SwapDirection::Add => quote_asset_reserve_before
            .checked_sub(quote_asset_reserve_after)
            .ok_or_else(math_error!())?,

        SwapDirection::Remove => quote_asset_reserve_after
            .checked_sub(quote_asset_reserve_before)
            .ok_or_else(math_error!())?,
    };

    let mut quote_asset_amount =
        reserve_to_asset_amount(quote_asset_reserve_change, peg_multiplier)?;

    // when a user goes long base asset, make the base asset slightly more expensive
    // by adding one unit of quote asset
    if swap_direction == SwapDirection::Remove {
        quote_asset_amount = quote_asset_amount
            .checked_add(1)
            .ok_or_else(math_error!())?;
    }

    Ok(quote_asset_amount)
}

pub fn calculate_terminal_reserves(amm: &AMM) -> ClearingHouseResult<(u128, u128)> {
    let swap_direction = if amm.net_base_asset_amount > 0 {
        SwapDirection::Add
    } else {
        SwapDirection::Remove
    };
    let (new_quote_asset_amount, new_base_asset_amount) = calculate_swap_output(
        amm.net_base_asset_amount.unsigned_abs(),
        amm.base_asset_reserve,
        swap_direction,
        amm.sqrt_k,
    )?;

    Ok((new_quote_asset_amount, new_base_asset_amount))
}

pub fn calculate_terminal_price_and_reserves(amm: &AMM) -> ClearingHouseResult<(u128, u128, u128)> {
    let (new_quote_asset_amount, new_base_asset_amount) = calculate_terminal_reserves(amm)?;

    let terminal_price = calculate_price(
        new_quote_asset_amount,
        new_base_asset_amount,
        amm.peg_multiplier,
    )?;

    Ok((
        terminal_price,
        new_quote_asset_amount,
        new_base_asset_amount,
    ))
}

pub fn get_spread_reserves(
    amm: &AMM,
    direction: PositionDirection,
) -> ClearingHouseResult<(u128, u128)> {
    let (base_asset_reserve, quote_asset_reserve) = match direction {
        PositionDirection::Long => (amm.ask_base_asset_reserve, amm.ask_quote_asset_reserve),
        PositionDirection::Short => (amm.bid_base_asset_reserve, amm.bid_quote_asset_reserve),
    };

    Ok((base_asset_reserve, quote_asset_reserve))
}

pub fn calculate_spread_reserves(
    amm: &AMM,
    direction: PositionDirection,
) -> ClearingHouseResult<(u128, u128)> {
    let spread = match direction {
        PositionDirection::Long => amm.long_spread,
        PositionDirection::Short => amm.short_spread,
    };

    let quote_asset_reserve_delta = if spread > 0 {
        amm.quote_asset_reserve
            .checked_div(BID_ASK_SPREAD_PRECISION / (spread / 2))
            .ok_or_else(math_error!())?
    } else {
        0
    };

    let quote_asset_reserve = match direction {
        PositionDirection::Long => amm
            .quote_asset_reserve
            .checked_add(quote_asset_reserve_delta)
            .ok_or_else(math_error!())?,
        PositionDirection::Short => amm
            .quote_asset_reserve
            .checked_sub(quote_asset_reserve_delta)
            .ok_or_else(math_error!())?,
    };

    let invariant_sqrt_u192 = U192::from(amm.sqrt_k);
    let invariant = invariant_sqrt_u192
        .checked_mul(invariant_sqrt_u192)
        .ok_or_else(math_error!())?;

    let base_asset_reserve = invariant
        .checked_div(U192::from(quote_asset_reserve))
        .ok_or_else(math_error!())?
        .try_to_u128()?;

    Ok((base_asset_reserve, quote_asset_reserve))
}

pub fn calculate_oracle_reserve_price_spread(
    amm: &AMM,
    oracle_price_data: &OraclePriceData,
    precomputed_reserve_price: Option<u128>,
) -> ClearingHouseResult<(i128, i128)> {
    let reserve_price = match precomputed_reserve_price {
        Some(reserve_price) => cast_to_i128(reserve_price)?,
        None => cast_to_i128(amm.reserve_price()?)?,
    };

    let oracle_price = oracle_price_data.price;

    let price_spread = reserve_price
        .checked_sub(oracle_price)
        .ok_or_else(math_error!())?;

    Ok((oracle_price, price_spread))
}

pub fn normalise_oracle_price(
    amm: &AMM,
    oracle_price: &OraclePriceData,
    precomputed_reserve_price: Option<u128>,
) -> ClearingHouseResult<i128> {
    let OraclePriceData {
        price: oracle_price,
        confidence: oracle_conf,
        ..
    } = *oracle_price;

    let reserve_price = match precomputed_reserve_price {
        Some(reserve_price) => cast_to_i128(reserve_price)?,
        None => cast_to_i128(amm.reserve_price()?)?,
    };

    // 2.5 bps of the mark price
    let reserve_price_2p5_bps = reserve_price.checked_div(4000).ok_or_else(math_error!())?;
    let conf_int = cast_to_i128(oracle_conf)?;

    //  normalises oracle toward mark price based on the oracle’s confidence interval
    //  if mark above oracle: use oracle+conf unless it exceeds .99975 * mark price
    //  if mark below oracle: use oracle-conf unless it less than 1.00025 * mark price
    //  (this guarantees more reasonable funding rates in volatile periods)
    let normalised_price = if reserve_price > oracle_price {
        min(
            max(
                reserve_price
                    .checked_sub(reserve_price_2p5_bps)
                    .ok_or_else(math_error!())?,
                oracle_price,
            ),
            oracle_price
                .checked_add(conf_int)
                .ok_or_else(math_error!())?,
        )
    } else {
        max(
            min(
                reserve_price
                    .checked_add(reserve_price_2p5_bps)
                    .ok_or_else(math_error!())?,
                oracle_price,
            ),
            oracle_price
                .checked_sub(conf_int)
                .ok_or_else(math_error!())?,
        )
    };

    Ok(normalised_price)
}

pub fn calculate_oracle_reserve_price_spread_pct(
    amm: &AMM,
    oracle_price_data: &OraclePriceData,
    precomputed_reserve_price: Option<u128>,
) -> ClearingHouseResult<i128> {
    let reserve_price = match precomputed_reserve_price {
        Some(reserve_price) => reserve_price,
        None => amm.reserve_price()?,
    };
    let (_oracle_price, price_spread) =
        calculate_oracle_reserve_price_spread(amm, oracle_price_data, Some(reserve_price))?;

    price_spread
        .checked_mul(BID_ASK_SPREAD_PRECISION_I128)
        .ok_or_else(math_error!())?
        .checked_div(cast_to_i128(reserve_price)?) // todo? better for spread logic
        .ok_or_else(math_error!())
}

pub fn calculate_oracle_twap_5min_mark_spread_pct(
    amm: &AMM,
    precomputed_reserve_price: Option<u128>,
) -> ClearingHouseResult<i128> {
    let reserve_price = match precomputed_reserve_price {
        Some(reserve_price) => reserve_price,
        None => amm.reserve_price()?,
    };
    let price_spread = cast_to_i128(reserve_price)?
        .checked_sub(amm.historical_oracle_data.last_oracle_price_twap_5min)
        .ok_or_else(math_error!())?;

    // price_spread_pct
    price_spread
        .checked_mul(BID_ASK_SPREAD_PRECISION_I128)
        .ok_or_else(math_error!())?
        .checked_div(cast_to_i128(reserve_price)?) // todo? better for spread logic
        .ok_or_else(math_error!())
}

pub fn is_oracle_mark_too_divergent(
    price_spread_pct: i128,
    oracle_guard_rails: &PriceDivergenceGuardRails,
) -> ClearingHouseResult<bool> {
    let max_divergence = oracle_guard_rails
        .mark_oracle_divergence_numerator
        .checked_mul(BID_ASK_SPREAD_PRECISION)
        .ok_or_else(math_error!())?
        .checked_div(oracle_guard_rails.mark_oracle_divergence_denominator)
        .ok_or_else(math_error!())?;

    Ok(price_spread_pct.unsigned_abs() > max_divergence)
}

pub fn calculate_mark_twap_spread_pct(amm: &AMM, reserve_price: u128) -> ClearingHouseResult<i128> {
    let reserve_price = cast_to_i128(reserve_price)?;
    let mark_twap = cast_to_i128(amm.last_mark_price_twap)?;

    let price_spread = reserve_price
        .checked_sub(mark_twap)
        .ok_or_else(math_error!())?;

    price_spread
        .checked_mul(BID_ASK_SPREAD_PRECISION_I128)
        .ok_or_else(math_error!())?
        .checked_div(mark_twap)
        .ok_or_else(math_error!())
}

pub fn use_oracle_price_for_margin_calculation(
    price_spread_pct: i128,
    oracle_guard_rails: &PriceDivergenceGuardRails,
) -> ClearingHouseResult<bool> {
    let max_divergence = oracle_guard_rails
        .mark_oracle_divergence_numerator
        .checked_mul(BID_ASK_SPREAD_PRECISION)
        .ok_or_else(math_error!())?
        .checked_div(oracle_guard_rails.mark_oracle_divergence_denominator)
        .ok_or_else(math_error!())?
        .checked_div(3)
        .ok_or_else(math_error!())?;

    Ok(price_spread_pct.unsigned_abs() > max_divergence)
}

pub fn calculate_budgeted_k_scale(
    market: &mut PerpMarket,
    budget: i128,
    increase_max: i128,
) -> ClearingHouseResult<(u128, u128)> {
    let curve_update_intensity = market.amm.curve_update_intensity as i128;
    let k_pct_upper_bound = increase_max;

    validate!(
        increase_max >= K_BPS_UPDATE_SCALE,
        ErrorCode::DefaultError,
        "invalid increase_max={} < {}",
        increase_max,
        K_BPS_UPDATE_SCALE
    )?;

    let k_pct_lower_bound =
        K_BPS_UPDATE_SCALE - (K_BPS_DECREASE_MAX) * curve_update_intensity / 100;

    let (numerator, denominator) = _calculate_budgeted_k_scale(
        market.amm.base_asset_reserve,
        market.amm.quote_asset_reserve,
        budget,
        market.amm.peg_multiplier,
        market.amm.net_base_asset_amount,
        k_pct_upper_bound,
        k_pct_lower_bound,
    )?;

    Ok((numerator, denominator))
}

pub fn _calculate_budgeted_k_scale(
    x: u128,
    y: u128,
    budget: i128,
    q: u128,
    d: i128,
    k_pct_upper_bound: i128,
    k_pct_lower_bound: i128,
) -> ClearingHouseResult<(u128, u128)> {
    // let curve_update_intensity = curve_update_intensity as i128;
    let c = -budget;
    let q = cast_to_i128(q)?;

    let c_sign: i128 = if c > 0 { 1 } else { -1 };
    let d_sign: i128 = if d > 0 { 1 } else { -1 };

    let x_d = cast_to_i128(x)?.checked_add(d).ok_or_else(math_error!())?;

    let amm_reserve_precision_u192 = U192::from(AMM_RESERVE_PRECISION);
    let x_times_x_d_u192 = U192::from(x)
        .checked_mul(U192::from(x_d))
        .ok_or_else(math_error!())?
        .checked_div(amm_reserve_precision_u192)
        .ok_or_else(math_error!())?;

    let quote_precision_u192 = U192::from(QUOTE_PRECISION);
    let x_times_x_d_c = x_times_x_d_u192
        .checked_mul(U192::from(c.unsigned_abs()))
        .ok_or_else(math_error!())?
        .checked_div(quote_precision_u192)
        .ok_or_else(math_error!())?
        .try_to_u128()?;

    let c_times_x_d_d = U192::from(c.unsigned_abs())
        .checked_mul(U192::from(x_d.unsigned_abs()))
        .ok_or_else(math_error!())?
        .checked_div(quote_precision_u192)
        .ok_or_else(math_error!())?
        .checked_mul(U192::from(d.unsigned_abs()))
        .ok_or_else(math_error!())?
        .checked_div(amm_reserve_precision_u192)
        .ok_or_else(math_error!())?
        .try_to_u128()?;

    let pegged_quote_times_dd = cast_to_i128(
        U192::from(y)
            .checked_mul(U192::from(d.unsigned_abs()))
            .ok_or_else(math_error!())?
            .checked_div(amm_reserve_precision_u192)
            .ok_or_else(math_error!())?
            .checked_mul(U192::from(d.unsigned_abs()))
            .ok_or_else(math_error!())?
            .checked_div(amm_reserve_precision_u192)
            .ok_or_else(math_error!())?
            .checked_mul(U192::from(q))
            .ok_or_else(math_error!())?
            .checked_div(U192::from(PEG_PRECISION))
            .ok_or_else(math_error!())?
            .try_to_u128()?,
    )?;

    let numer1 = pegged_quote_times_dd;

    let numer2 = cast_to_i128(c_times_x_d_d)?
        .checked_mul(c_sign.checked_mul(d_sign).ok_or_else(math_error!())?)
        .ok_or_else(math_error!())?;

    let denom1 = cast_to_i128(x_times_x_d_c)?
        .checked_mul(c_sign)
        .ok_or_else(math_error!())?;

    let denom2 = pegged_quote_times_dd;

    // protocol is spending to increase k
    if c_sign < 0 {
        // thus denom1 is negative and solution is unstable
        if x_times_x_d_c > pegged_quote_times_dd.unsigned_abs() {
            msg!("cost exceeds possible amount to spend");
            msg!("k * {:?}/{:?}", k_pct_upper_bound, K_BPS_UPDATE_SCALE);
            return Ok((
                cast_to_u128(k_pct_upper_bound)?,
                cast_to_u128(K_BPS_UPDATE_SCALE)?,
            ));
        }
    }

    let mut numerator = (numer1.checked_sub(numer2).ok_or_else(math_error!())?)
        .checked_div(AMM_TO_QUOTE_PRECISION_RATIO_I128)
        .ok_or_else(math_error!())?;
    let mut denominator = denom1
        .checked_add(denom2)
        .ok_or_else(math_error!())?
        .checked_div(AMM_TO_QUOTE_PRECISION_RATIO_I128)
        .ok_or_else(math_error!())?;

    if numerator < 0 && denominator < 0 {
        numerator = numerator.abs();
        denominator = denominator.abs();
    }
    assert!((numerator > 0 && denominator > 0));

    let (numerator, denominator) = if numerator > denominator {
        let current_pct_change = numerator
            .checked_mul(10000)
            .ok_or_else(math_error!())?
            .checked_div(denominator)
            .ok_or_else(math_error!())?;

        let maximum_pct_change = k_pct_upper_bound
            .checked_mul(10000)
            .ok_or_else(math_error!())?
            .checked_div(K_BPS_UPDATE_SCALE)
            .ok_or_else(math_error!())?;

        if current_pct_change > maximum_pct_change {
            (k_pct_upper_bound, K_BPS_UPDATE_SCALE)
        } else {
            (numerator, denominator)
        }
    } else {
        let current_pct_change = numerator
            .checked_mul(10000)
            .ok_or_else(math_error!())?
            .checked_div(denominator)
            .ok_or_else(math_error!())?;

        let maximum_pct_change = k_pct_lower_bound
            .checked_mul(10000)
            .ok_or_else(math_error!())?
            .checked_div(K_BPS_UPDATE_SCALE)
            .ok_or_else(math_error!())?;

        if current_pct_change < maximum_pct_change {
            (k_pct_lower_bound, K_BPS_UPDATE_SCALE)
        } else {
            (numerator, denominator)
        }
    };

    Ok((cast_to_u128(numerator)?, cast_to_u128(denominator)?))
}

/// To find the cost of adjusting k, compare the the net market value before and after adjusting k
/// Increasing k costs the protocol money because it reduces slippage and improves the exit price for net market position
/// Decreasing k costs the protocol money because it increases slippage and hurts the exit price for net market position
pub fn adjust_k_cost(
    market: &mut PerpMarket,
    update_k_result: &UpdateKResult,
) -> ClearingHouseResult<i128> {
    let mut market_clone = *market;

    // Find the net market value before adjusting k
    let (current_net_market_value, _) = _calculate_base_asset_value_and_pnl(
        market_clone.amm.net_base_asset_amount,
        0,
        &market_clone.amm,
        false,
    )?;

    update_k(&mut market_clone, update_k_result)?;

    let (_new_net_market_value, cost) = _calculate_base_asset_value_and_pnl(
        market_clone.amm.net_base_asset_amount,
        current_net_market_value,
        &market_clone.amm,
        false,
    )?;

    Ok(cost)
}

/// To find the cost of adjusting k, compare the the net market value before and after adjusting k
/// Increasing k costs the protocol money because it reduces slippage and improves the exit price for net market position
/// Decreasing k costs the protocol money because it increases slippage and hurts the exit price for net market position
pub fn adjust_k_cost_and_update(
    market: &mut PerpMarket,
    update_k_result: &UpdateKResult,
) -> ClearingHouseResult<i128> {
    // Find the net market value before adjusting k
    let current_net_market_value =
        calculate_base_asset_value(market.amm.net_base_asset_amount, &market.amm, false)?;

    update_k(market, update_k_result)?;

    let (_new_net_market_value, cost) = _calculate_base_asset_value_and_pnl(
        market.amm.net_base_asset_amount,
        current_net_market_value,
        &market.amm,
        false,
    )?;

    Ok(cost)
}

pub struct UpdateKResult {
    pub sqrt_k: u128,
    pub base_asset_reserve: u128,
    pub quote_asset_reserve: u128,
}

pub fn get_update_k_result(
    market: &PerpMarket,
    new_sqrt_k: bn::U192,
    bound_update: bool,
) -> ClearingHouseResult<UpdateKResult> {
    let sqrt_k_ratio_precision = bn::U192::from(AMM_RESERVE_PRECISION);

    let old_sqrt_k = bn::U192::from(market.amm.sqrt_k);
    let mut sqrt_k_ratio = new_sqrt_k
        .checked_mul(sqrt_k_ratio_precision)
        .ok_or_else(math_error!())?
        .checked_div(old_sqrt_k)
        .ok_or_else(math_error!())?;

    // if decreasing k, max decrease ratio for single transaction is 2.5%
    if bound_update && sqrt_k_ratio < U192::from(975_000_000_u128) {
        return Err(ErrorCode::InvalidUpdateK);
    }

    if sqrt_k_ratio < sqrt_k_ratio_precision {
        sqrt_k_ratio = sqrt_k_ratio + 1;
    }

    let sqrt_k = new_sqrt_k.try_to_u128().unwrap();

    if bound_update
        && new_sqrt_k < old_sqrt_k
        && market.amm.net_base_asset_amount.unsigned_abs()
            > sqrt_k.checked_div(3).ok_or_else(math_error!())?
    {
        // todo, check less lp_tokens as well
        msg!("new_sqrt_k too small relative to market imbalance");
        return Err(ErrorCode::InvalidUpdateK);
    }

    if market.amm.net_base_asset_amount.unsigned_abs() > sqrt_k {
        msg!("new_sqrt_k too small relative to market imbalance");
        return Err(ErrorCode::InvalidUpdateK);
    }

    let base_asset_reserve = bn::U192::from(market.amm.base_asset_reserve)
        .checked_mul(sqrt_k_ratio)
        .ok_or_else(math_error!())?
        .checked_div(sqrt_k_ratio_precision)
        .ok_or_else(math_error!())?
        .try_to_u128()?;

    let invariant_sqrt_u192 = U192::from(sqrt_k);
    let invariant = invariant_sqrt_u192
        .checked_mul(invariant_sqrt_u192)
        .ok_or_else(math_error!())?;

    let quote_asset_reserve = invariant
        .checked_div(U192::from(base_asset_reserve))
        .ok_or_else(math_error!())?
        .try_to_u128()?;

    Ok(UpdateKResult {
        sqrt_k,
        base_asset_reserve,
        quote_asset_reserve,
    })
}

pub fn update_k(market: &mut PerpMarket, update_k_result: &UpdateKResult) -> ClearingHouseResult {
    market.amm.base_asset_reserve = update_k_result.base_asset_reserve;
    market.amm.quote_asset_reserve = update_k_result.quote_asset_reserve;
    market.amm.sqrt_k = update_k_result.sqrt_k;

    let (new_terminal_quote_reserve, new_terminal_base_reserve) =
        calculate_terminal_reserves(&market.amm)?;
    market.amm.terminal_quote_asset_reserve = new_terminal_quote_reserve;

    let (min_base_asset_reserve, max_base_asset_reserve) =
        calculate_bid_ask_bounds(market.amm.concentration_coef, new_terminal_base_reserve)?;
    market.amm.min_base_asset_reserve = min_base_asset_reserve;
    market.amm.max_base_asset_reserve = max_base_asset_reserve;

    let reserve_price_after = market.amm.reserve_price()?;
    crate::controller::amm::update_spreads(&mut market.amm, reserve_price_after)?;

    Ok(())
}

pub fn calculate_base_asset_amount_to_trade_to_price(
    amm: &AMM,
    limit_price: u128,
    direction: PositionDirection,
) -> ClearingHouseResult<(u128, PositionDirection)> {
    let invariant_sqrt_u192 = U192::from(amm.sqrt_k);
    let invariant = invariant_sqrt_u192
        .checked_mul(invariant_sqrt_u192)
        .ok_or_else(math_error!())?;

    validate!(limit_price > 0, ErrorCode::DefaultError, "limit_price <= 0")?;

    let new_base_asset_reserve_squared = invariant
        .checked_mul(U192::from(PRICE_PRECISION))
        .ok_or_else(math_error!())?
        .checked_div(U192::from(limit_price))
        .ok_or_else(math_error!())?
        .checked_mul(U192::from(amm.peg_multiplier))
        .ok_or_else(math_error!())?
        .checked_div(U192::from(PEG_PRECISION))
        .ok_or_else(math_error!())?;

    let new_base_asset_reserve = new_base_asset_reserve_squared
        .integer_sqrt()
        .try_to_u128()?;

    let base_asset_reserve_before = if amm.base_spread > 0 {
        let (spread_base_asset_reserve, _) = get_spread_reserves(amm, direction)?;
        spread_base_asset_reserve
    } else {
        amm.base_asset_reserve
    };

    if new_base_asset_reserve > base_asset_reserve_before {
        let max_trade_amount = new_base_asset_reserve
            .checked_sub(base_asset_reserve_before)
            .ok_or_else(math_error!())?;
        Ok((max_trade_amount, PositionDirection::Short))
    } else {
        let max_trade_amount = base_asset_reserve_before
            .checked_sub(new_base_asset_reserve)
            .ok_or_else(math_error!())?;
        Ok((max_trade_amount, PositionDirection::Long))
    }
}

pub fn calculate_max_base_asset_amount_fillable(
    amm: &AMM,
    order_direction: &PositionDirection,
) -> ClearingHouseResult<u128> {
    let max_fill_size = amm.base_asset_reserve / amm.max_base_asset_amount_ratio as u128;

    // one fill can only take up to half of side's liquidity
    let max_base_asset_amount_on_side = match order_direction {
        PositionDirection::Long => {
            amm.base_asset_reserve
                .saturating_sub(amm.min_base_asset_reserve)
                / 2
        }
        PositionDirection::Short => {
            amm.max_base_asset_reserve
                .saturating_sub(amm.base_asset_reserve)
                / 2
        }
    };

    standardize_base_asset_amount(
        max_fill_size.min(max_base_asset_amount_on_side),
        amm.base_asset_amount_step_size,
    )
}

pub fn calculate_net_user_cost_basis(amm: &AMM) -> ClearingHouseResult<i128> {
    amm.quote_asset_amount_long
        .checked_add(amm.quote_asset_amount_short)
        .ok_or_else(math_error!())?
        .checked_sub(amm.cumulative_social_loss)
        .ok_or_else(math_error!())
}

pub fn calculate_net_user_pnl(amm: &AMM, oracle_price: i128) -> ClearingHouseResult<i128> {
    validate!(
        oracle_price > 0,
        ErrorCode::DefaultError,
        "oracle_price <= 0",
    )?;

    let net_user_base_asset_value = amm
        .net_base_asset_amount
        .checked_mul(oracle_price)
        .ok_or_else(math_error!())?
        .checked_div(AMM_RESERVE_PRECISION_I128 * cast_to_i128(PRICE_TO_QUOTE_PRECISION_RATIO)?)
        .ok_or_else(math_error!())?;

    net_user_base_asset_value
        .checked_add(calculate_net_user_cost_basis(amm)?)
        .ok_or_else(math_error!())
}

pub fn calculate_settlement_price(
    amm: &AMM,
    target_price: i128,
    pnl_pool_amount: u128,
) -> ClearingHouseResult<i128> {
    if amm.net_base_asset_amount == 0 {
        return Ok(target_price);
    }

    // net_baa * price + net_quote <= 0
    // net_quote/net_baa <= -price

    // net_user_unrealized_pnl negative = surplus in market
    // net_user_unrealized_pnl positive = settlement price needs to differ from oracle
    let best_settlement_price = -(amm
        .quote_asset_amount_long
        .checked_add(amm.quote_asset_amount_short)
        .ok_or_else(math_error!())?
        .checked_sub(cast_to_i128(pnl_pool_amount)?)
        .ok_or_else(math_error!())?
        .checked_mul(AMM_RESERVE_PRECISION_I128 * cast_to_i128(PRICE_TO_QUOTE_PRECISION_RATIO)?)
        .ok_or_else(math_error!())?
        .checked_div(amm.net_base_asset_amount)
        .ok_or_else(math_error!())?);

    let settlement_price = if amm.net_base_asset_amount > 0 {
        // net longs only get as high as oracle_price
        best_settlement_price
            .min(target_price)
            .checked_sub(1)
            .ok_or_else(math_error!())?
    } else {
        // net shorts only get as low as oracle price
        best_settlement_price
            .max(target_price)
            .checked_add(1)
            .ok_or_else(math_error!())?
    };

    Ok(settlement_price)
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::controller::amm::update_spreads;
    use crate::controller::lp::burn_lp_shares;
    use crate::controller::lp::mint_lp_shares;
    use crate::controller::lp::settle_lp_position;
    use crate::math::constants::{
        BID_ASK_SPREAD_PRECISION, K_BPS_INCREASE_MAX, MAX_CONCENTRATION_COEFFICIENT,
        PRICE_PRECISION, QUOTE_PRECISION_I128,
    };
    use crate::state::oracle::HistoricalOracleData;
    use crate::state::user::PerpPosition;

    #[test]
    fn calculate_net_user_pnl_test() {
        let prev = 1656682258;
        let _now = prev + 3600;

        let px = 32 * PRICE_PRECISION;

        let mut amm = AMM {
            base_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            quote_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            peg_multiplier: PEG_PRECISION,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: px as i128,
                last_oracle_price_twap_ts: prev,

                ..HistoricalOracleData::default()
            },
            mark_std: PRICE_PRECISION as u64,
            last_mark_price_twap_ts: prev,
            funding_period: 3600_i64,
            ..AMM::default_test()
        };

        let oracle_price_data = OraclePriceData {
            price: (34 * PRICE_PRECISION) as i128,
            confidence: PRICE_PRECISION / 100,
            delay: 1,
            has_sufficient_number_of_data_points: true,
        };

        let net_user_pnl = calculate_net_user_pnl(&amm, oracle_price_data.price).unwrap();
        assert_eq!(net_user_pnl, 0);

        amm.cumulative_social_loss = -QUOTE_PRECISION_I128;
        let net_user_pnl = calculate_net_user_pnl(&amm, oracle_price_data.price).unwrap();
        assert_eq!(net_user_pnl, QUOTE_PRECISION_I128);

        let market = PerpMarket::default_btc_test();
        let net_user_pnl = calculate_net_user_pnl(
            &market.amm,
            market.amm.historical_oracle_data.last_oracle_price,
        )
        .unwrap();
        assert_eq!(net_user_pnl, -400000000); // down $400

        let net_user_pnl =
            calculate_net_user_pnl(&market.amm, 17501 * PRICE_PRECISION_I128).unwrap();
        assert_eq!(net_user_pnl, 1499000000); // up $1499
    }

    #[test]
    fn calculate_settlement_price_long_imbalance_with_loss_test() {
        let prev = 1656682258;
        let _now = prev + 3600;

        // imbalanced short, no longs
        // btc
        let oracle_price_data = OraclePriceData {
            price: (22050 * PRICE_PRECISION) as i128,
            confidence: 0,
            delay: 2,
            has_sufficient_number_of_data_points: true,
        };

        let market_position = PerpPosition {
            market_index: 0,
            base_asset_amount: (12295081967 / 2_i128),
            quote_asset_amount: -193688524588, // $31506 entry price
            ..PerpPosition::default()
        };

        let market = PerpMarket {
            market_index: 0,
            amm: AMM {
                base_asset_reserve: 512295081967,
                quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
                sqrt_k: 500 * AMM_RESERVE_PRECISION,
                peg_multiplier: 22_100_000_000,
                net_base_asset_amount: (12295081967_i128),
                max_spread: 1000,
                quote_asset_amount_long: market_position.quote_asset_amount * 2,
                // assume someone else has other half same entry,
                ..AMM::default()
            },
            margin_ratio_initial: 1000,
            margin_ratio_maintenance: 500,
            imf_factor: 1000, // 1_000/1_000_000 = .001
            unrealized_initial_asset_weight: 100,
            unrealized_maintenance_asset_weight: 100,
            ..PerpMarket::default()
        };

        let mut settlement_price =
            calculate_settlement_price(&market.amm, oracle_price_data.price, 0).unwrap();

        let reserve_price = market.amm.reserve_price().unwrap();
        let (terminal_price, _, _) = calculate_terminal_price_and_reserves(&market.amm).unwrap();
        let oracle_price = oracle_price_data.price;

        assert_eq!(settlement_price, 22049999999);
        assert_eq!(terminal_price, 20076684570);
        assert_eq!(oracle_price, 22050000000);
        assert_eq!(reserve_price, 21051929600);

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            111_111_110, // $111
        )
        .unwrap();

        assert_eq!(settlement_price, 22049999999); // same price

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            1_111_111_110, // $1,111
        )
        .unwrap();

        assert_eq!(settlement_price, 22049999999); // same price again

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            111_111_110 * QUOTE_PRECISION,
        )
        .unwrap();

        assert_eq!(settlement_price, 22049999999);
        assert_eq!(settlement_price, oracle_price - 1); // more longs than shorts, bias = -1
    }

    #[test]
    fn calculate_settlement_price_long_imbalance_test() {
        let prev = 1656682258;
        let _now = prev + 3600;

        // imbalanced short, no longs
        // btc
        let oracle_price_data = OraclePriceData {
            price: (22050 * PRICE_PRECISION) as i128,
            confidence: 0,
            delay: 2,
            has_sufficient_number_of_data_points: true,
        };

        let market_position = PerpPosition {
            market_index: 0,
            base_asset_amount: (12295081967 / 2_i128),
            quote_asset_amount: -103688524588, // $16,866.66 entry price
            ..PerpPosition::default()
        };

        let market = PerpMarket {
            market_index: 0,
            amm: AMM {
                base_asset_reserve: 512295081967,
                quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
                sqrt_k: 500 * AMM_RESERVE_PRECISION,
                peg_multiplier: 22_100_000_000,
                net_base_asset_amount: (12295081967_i128),
                max_spread: 1000,
                quote_asset_amount_long: market_position.quote_asset_amount * 2,
                // assume someone else has other half same entry,
                ..AMM::default()
            },
            margin_ratio_initial: 1000,
            margin_ratio_maintenance: 500,
            imf_factor: 1000, // 1_000/1_000_000 = .001
            unrealized_initial_asset_weight: 100,
            unrealized_maintenance_asset_weight: 100,
            ..PerpMarket::default()
        };

        let mut settlement_price =
            calculate_settlement_price(&market.amm, oracle_price_data.price, 0).unwrap();

        let reserve_price = market.amm.reserve_price().unwrap();
        let (terminal_price, _, _) = calculate_terminal_price_and_reserves(&market.amm).unwrap();
        let oracle_price = oracle_price_data.price;

        assert_eq!(settlement_price, 16866666665);
        assert_eq!(terminal_price, 20076684570);
        assert_eq!(oracle_price, 22050000000);
        assert_eq!(reserve_price, 21051929600);

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            111_111_110, // $111
        )
        .unwrap();

        assert_eq!(settlement_price, 16875703702); // better price

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            1_111_111_110, // $1,111
        )
        .unwrap();

        assert_eq!(settlement_price, 16957037035); // even better price

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            111_111_110 * QUOTE_PRECISION,
        )
        .unwrap();

        assert_eq!(settlement_price, 22049999999);
        assert_eq!(settlement_price, oracle_price - 1); // more longs than shorts, bias = -1
    }

    #[test]
    fn calculate_settlement_price_test() {
        let prev = 1656682258;
        let _now = prev + 3600;

        let px = 32 * PRICE_PRECISION;

        let amm = AMM {
            base_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            quote_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            peg_multiplier: PEG_PRECISION,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: px as i128,
                last_oracle_price_twap_ts: prev,

                ..HistoricalOracleData::default()
            },
            mark_std: PRICE_PRECISION as u64,
            last_mark_price_twap_ts: prev,
            funding_period: 3600_i64,
            ..AMM::default_test()
        };

        let oracle_price_data = OraclePriceData {
            price: (34 * PRICE_PRECISION) as i128,
            confidence: PRICE_PRECISION / 100,
            delay: 1,
            has_sufficient_number_of_data_points: true,
        };

        let mut settlement_price =
            calculate_settlement_price(&amm, oracle_price_data.price, 0).unwrap();

        assert_eq!(settlement_price, oracle_price_data.price);

        settlement_price =
            calculate_settlement_price(&amm, oracle_price_data.price, 111111110).unwrap();

        assert_eq!(settlement_price, oracle_price_data.price);

        // imbalanced short, no longs
        // btc
        let oracle_price_data = OraclePriceData {
            price: (22050 * PRICE_PRECISION) as i128,
            confidence: 0,
            delay: 2,
            has_sufficient_number_of_data_points: true,
        };

        let market_position = PerpPosition {
            market_index: 0,
            base_asset_amount: -(122950819670000 / 2_i128),
            quote_asset_amount: 153688524588, // $25,000 entry price
            ..PerpPosition::default()
        };

        let market = PerpMarket {
            market_index: 0,
            amm: AMM {
                base_asset_reserve: 512295081967,
                quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
                sqrt_k: 500 * AMM_RESERVE_PRECISION,
                peg_multiplier: 22_100_000_000,
                net_base_asset_amount: -(12295081967_i128),
                max_spread: 1000,
                quote_asset_amount_short: market_position.quote_asset_amount * 2,
                // assume someone else has other half same entry,
                ..AMM::default()
            },
            margin_ratio_initial: 1000,
            margin_ratio_maintenance: 500,
            imf_factor: 1000, // 1_000/1_000_000 = .001
            unrealized_initial_asset_weight: 100,
            unrealized_maintenance_asset_weight: 100,
            ..PerpMarket::default()
        };

        let mut settlement_price =
            calculate_settlement_price(&market.amm, oracle_price_data.price, 0).unwrap();

        let reserve_price = market.amm.reserve_price().unwrap();
        let (terminal_price, _, _) = calculate_terminal_price_and_reserves(&market.amm).unwrap();
        let oracle_price = oracle_price_data.price;

        assert_eq!(settlement_price, 25000000001);
        assert_eq!(terminal_price, 22100000000);
        assert_eq!(oracle_price, 22050000000);
        assert_eq!(reserve_price, 21051929600);

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            111_111_110, // $111
        )
        .unwrap();

        // 250000000000814 - 249909629631346 = 90370369468 (~$9 improved)
        assert_eq!(settlement_price, 24990962964); // better price

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            1_111_111_110, // $1,111
        )
        .unwrap();

        // 250000000000814 - 249096296297998 = 903703702816 (~$90 improved)
        assert_eq!(settlement_price, 24909629630); // even better price

        settlement_price = calculate_settlement_price(
            &market.amm,
            oracle_price_data.price,
            111111110 * QUOTE_PRECISION,
        )
        .unwrap();

        assert_eq!(settlement_price, 22050000001);
        assert_eq!(settlement_price, oracle_price + 1); // more shorts than longs, bias = +1
    }

    #[test]
    fn max_spread_tests() {
        let (l, s) = cap_to_max_spread(3905832905, 3582930, 1000).unwrap();
        assert_eq!(l, 1000);
        assert_eq!(s, 0);

        let (l, s) = cap_to_max_spread(9999, 1, 1000).unwrap();
        assert_eq!(l, 1000);
        assert_eq!(s, 0);

        let (l, s) = cap_to_max_spread(999, 1, 1000).unwrap();
        assert_eq!(l, 999);
        assert_eq!(s, 1);

        let (l, s) = cap_to_max_spread(444, 222, 1000).unwrap();
        assert_eq!(l, 444);
        assert_eq!(s, 222);

        let (l, s) = cap_to_max_spread(150, 2221, 1000).unwrap();
        assert_eq!(l, 0);
        assert_eq!(s, 1000);

        let (l, s) = cap_to_max_spread(2500 - 10, 11, 2500).unwrap();
        assert_eq!(l, 2490);
        assert_eq!(s, 10);

        let (l, s) = cap_to_max_spread(2510, 110, 2500).unwrap();
        assert_eq!(l, 2500);
        assert_eq!(s, 0);
    }

    #[test]
    fn calculate_spread_tests() {
        let base_spread = 1000; // .1%
        let mut last_oracle_reserve_price_spread_pct = 0;
        let mut last_oracle_conf_pct = 0;
        let quote_asset_reserve = AMM_RESERVE_PRECISION * 10;
        let mut terminal_quote_asset_reserve = AMM_RESERVE_PRECISION * 10;
        let peg_multiplier = 34000000;
        let mut net_base_asset_amount = 0;
        let reserve_price = 34562304;
        let mut total_fee_minus_distributions = 0;

        let base_asset_reserve = AMM_RESERVE_PRECISION * 10;
        let min_base_asset_reserve = 0_u128;
        let max_base_asset_reserve = AMM_RESERVE_PRECISION * 100000;

        let margin_ratio_initial = 2000; // 5x max leverage
        let max_spread = margin_ratio_initial * 100;
        // at 0 fee be max spread
        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread1, (base_spread * 10 / 2) as u128);
        assert_eq!(short_spread1, (base_spread * 10 / 2) as u128);

        // even at imbalance with 0 fee, be max spread
        terminal_quote_asset_reserve -= AMM_RESERVE_PRECISION;
        net_base_asset_amount += AMM_RESERVE_PRECISION as i128;

        let (long_spread2, short_spread2) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread2, (base_spread * 10) as u128);
        assert_eq!(short_spread2, (base_spread * 10 / 2) as u128);

        // oracle retreat * skew that increases long spread
        last_oracle_reserve_price_spread_pct = BID_ASK_SPREAD_PRECISION_I128 / 20; //5%
        last_oracle_conf_pct = (BID_ASK_SPREAD_PRECISION / 100) as u64; //1%
        total_fee_minus_distributions = QUOTE_PRECISION as i128;
        let (long_spread3, short_spread3) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert!(short_spread3 > long_spread3);

        // 1000/2 * (1+(34562000-34000000)/QUOTE_PRECISION) -> 781
        assert_eq!(long_spread3, 1562);

        // last_oracle_reserve_price_spread_pct + conf retreat
        // assert_eq!(short_spread3, 1010000);
        assert_eq!(short_spread3, 60000); // hitting max spread

        last_oracle_reserve_price_spread_pct = -BID_ASK_SPREAD_PRECISION_I128 / 777;
        last_oracle_conf_pct = 1;
        let (long_spread4, short_spread4) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert!(short_spread4 < long_spread4);
        // (1000000/777 + 1 )* 1.562 * 2 -> 2012 * 2
        assert_eq!(long_spread4, 2012 * 2);
        // base_spread
        assert_eq!(short_spread4, 500);

        // increases to fee pool will decrease long spread (all else equal)
        let (long_spread5, short_spread5) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions * 2,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();

        assert!(long_spread5 < long_spread4);
        assert_eq!(short_spread5, short_spread4);

        let amm = AMM {
            base_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            quote_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            sqrt_k: 2 * AMM_RESERVE_PRECISION,
            peg_multiplier: PEG_PRECISION,
            long_spread: long_spread5,
            short_spread: short_spread5,
            ..AMM::default()
        };

        let (bar_l, qar_l) = calculate_spread_reserves(&amm, PositionDirection::Long).unwrap();
        let (bar_s, qar_s) = calculate_spread_reserves(&amm, PositionDirection::Short).unwrap();

        assert!(qar_l > amm.quote_asset_reserve);
        assert!(bar_l < amm.base_asset_reserve);
        assert!(qar_s < amm.quote_asset_reserve);
        assert!(bar_s > amm.base_asset_reserve);
        assert_eq!(bar_s, 2000500125);
        assert_eq!(bar_l, 1996705107);
        assert_eq!(qar_l, 2003300330);
        assert_eq!(qar_s, 1999500000);

        let (long_spread_btc, short_spread_btc) = calculate_spread(
            500,
            62099,
            411,
            margin_ratio_initial * 100,
            94280030695,
            94472846843,
            21966868000,
            -193160000,
            21927763871,
            50457675,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();

        assert_eq!(long_spread_btc, 500 / 2);
        assert_eq!(short_spread_btc, 74584);

        let (long_spread_btc1, short_spread_btc1) = calculate_spread(
            500,
            70719,
            0,
            margin_ratio_initial * 100,
            92113762421,
            92306488219,
            21754071000,
            -193060000,
            21671071573,
            4876326,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();

        assert_eq!(long_spread_btc1, 0);
        assert_eq!(short_spread_btc1, 200000); // max spread
    }

    #[test]
    fn calculate_spread_inventory_tests() {
        let base_spread = 1000; // .1%
        let last_oracle_reserve_price_spread_pct = 0;
        let last_oracle_conf_pct = 0;
        let quote_asset_reserve = AMM_RESERVE_PRECISION * 9;
        let mut terminal_quote_asset_reserve = AMM_RESERVE_PRECISION * 10;
        let peg_multiplier = 34000000;
        let mut net_base_asset_amount = -(AMM_RESERVE_PRECISION as i128);
        let reserve_price = 34562304;
        let mut total_fee_minus_distributions = 10000 * QUOTE_PRECISION_I128;

        let base_asset_reserve = AMM_RESERVE_PRECISION * 11;
        let min_base_asset_reserve = AMM_RESERVE_PRECISION * 7;
        let max_base_asset_reserve = AMM_RESERVE_PRECISION * 14;

        let margin_ratio_initial = 2000; // 5x max leverage
        let max_spread = margin_ratio_initial * 100;

        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();

        // inventory scale
        let (max_bids, max_asks) = _calculate_market_open_bids_asks(
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(max_bids, 4000000000);
        assert_eq!(max_asks, -3000000000);

        let total_liquidity = max_bids
            .checked_add(max_asks.abs())
            .ok_or_else(math_error!())
            .unwrap();
        assert_eq!(total_liquidity, 7000000000);
        // inventory scale
        let inventory_scale = net_base_asset_amount
            .checked_mul(BID_ASK_SPREAD_PRECISION_I128 * 5)
            .unwrap()
            .checked_div(total_liquidity)
            .unwrap()
            .unsigned_abs();
        assert_eq!(inventory_scale, 714285);

        assert_eq!(long_spread1, 500);
        assert_eq!(short_spread1, 2166);

        net_base_asset_amount *= 2;
        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread1, 500);
        assert_eq!(short_spread1, 3833);

        terminal_quote_asset_reserve = AMM_RESERVE_PRECISION * 11;
        total_fee_minus_distributions = QUOTE_PRECISION_I128 * 5;
        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price * 9 / 10,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread1, 500);
        assert_eq!(short_spread1, 8269);

        total_fee_minus_distributions = QUOTE_PRECISION_I128;
        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            net_base_asset_amount,
            reserve_price * 9 / 10,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread1, 500);
        assert_eq!(short_spread1, 26017); // 1214 * 5

        // flip sign
        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            -net_base_asset_amount,
            reserve_price * 9 / 10,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread1, 38330);
        assert_eq!(short_spread1, 500);

        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            -net_base_asset_amount * 5,
            reserve_price * 9 / 10,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve,
            max_base_asset_reserve,
        )
        .unwrap();
        assert_eq!(long_spread1, 50000);
        assert_eq!(short_spread1, 500);

        let (long_spread1, short_spread1) = calculate_spread(
            base_spread,
            last_oracle_reserve_price_spread_pct,
            last_oracle_conf_pct,
            max_spread,
            quote_asset_reserve,
            terminal_quote_asset_reserve,
            peg_multiplier,
            -net_base_asset_amount,
            reserve_price * 9 / 10,
            total_fee_minus_distributions,
            base_asset_reserve,
            min_base_asset_reserve / 2,
            max_base_asset_reserve * 2,
        )
        .unwrap();
        assert_eq!(long_spread1, 18330);
        assert_eq!(short_spread1, 500);
    }

    #[test]
    fn k_update_results_bound_flag() {
        let init_reserves = 100 * AMM_RESERVE_PRECISION;
        let amm = AMM {
            sqrt_k: init_reserves,
            base_asset_reserve: init_reserves,
            quote_asset_reserve: init_reserves,
            ..AMM::default()
        };
        let market = PerpMarket {
            amm,
            ..PerpMarket::default()
        };

        let new_sqrt_k = U192::from(AMM_RESERVE_PRECISION);
        let is_error = get_update_k_result(&market, new_sqrt_k, true).is_err();
        assert!(is_error);

        let is_ok = get_update_k_result(&market, new_sqrt_k, false).is_ok();
        assert!(is_ok)
    }

    #[test]
    fn calc_mark_std_tests() {
        let prev = 1656682258;
        let mut now = prev + 60;
        let mut amm = AMM {
            base_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            quote_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            peg_multiplier: PRICE_PRECISION,
            base_spread: 65535, //max base spread is 6.5%
            mark_std: PRICE_PRECISION as u64,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price: PRICE_PRECISION as i128,
                ..HistoricalOracleData::default()
            },
            last_mark_price_twap_ts: prev,
            ..AMM::default()
        };
        update_amm_mark_std(&mut amm, now, PRICE_PRECISION * 23, 0).unwrap();
        assert_eq!(amm.mark_std, 23000000);

        amm.mark_std = PRICE_PRECISION as u64;
        amm.last_mark_price_twap_ts = now - 60;
        update_amm_mark_std(&mut amm, now, PRICE_PRECISION * 2, 0).unwrap();
        assert_eq!(amm.mark_std, 2000000);

        let mut px = PRICE_PRECISION;
        let stop_time = now + 3600 * 2;
        while now <= stop_time {
            now += 1;
            if now % 15 == 0 {
                px = px * 1012 / 1000;
                amm.historical_oracle_data.last_oracle_price =
                    amm.historical_oracle_data.last_oracle_price * 10119 / 10000;
            } else {
                px = px * 100000 / 100133;
                amm.historical_oracle_data.last_oracle_price =
                    amm.historical_oracle_data.last_oracle_price * 100001 / 100133;
            }
            amm.peg_multiplier = px;
            let trade_direction = PositionDirection::Long;
            update_mark_twap(&mut amm, now, Some(px), Some(trade_direction)).unwrap();
        }
        assert_eq!(now, 1656689519);
        assert_eq!(px, 39397);
        assert_eq!(amm.mark_std, 105);

        // sol price looking thinkg
        let mut px: u128 = 31_936_658;
        let stop_time = now + 3600 * 2;
        while now <= stop_time {
            now += 1;
            if now % 15 == 0 {
                px = 31_986_658; //31.98
                amm.historical_oracle_data.last_oracle_price = (px - 1000000) as i128;
                amm.peg_multiplier = px;

                let trade_direction = PositionDirection::Long;
                update_mark_twap(&mut amm, now, Some(px), Some(trade_direction)).unwrap();
            }
            if now % 189 == 0 {
                px = 31_883_651; //31.88
                amm.peg_multiplier = px;

                amm.historical_oracle_data.last_oracle_price = (px + 1000000) as i128;
                let trade_direction = PositionDirection::Short;
                update_mark_twap(&mut amm, now, Some(px), Some(trade_direction)).unwrap();
            }
        }
        assert_eq!(now, 1656696720);
        assert_eq!(px, 31986658);
        assert_eq!(amm.mark_std, 384673);

        // sol price looking thinkg
        let mut px: u128 = 31_936_658;
        let stop_time = now + 3600 * 2;
        while now <= stop_time {
            now += 1;
            if now % 2 == 1 {
                px = 31_986_658; //31.98
                amm.peg_multiplier = px;

                amm.historical_oracle_data.last_oracle_price = (px - 1000000) as i128;
                let trade_direction = PositionDirection::Long;
                update_mark_twap(&mut amm, now, Some(px), Some(trade_direction)).unwrap();
            }
            if now % 2 == 0 {
                px = 31_883_651; //31.88
                amm.peg_multiplier = px;

                amm.historical_oracle_data.last_oracle_price = (px + 1000000) as i128;
                let trade_direction = PositionDirection::Short;
                update_mark_twap(&mut amm, now, Some(px), Some(trade_direction)).unwrap();
            }
        }
        assert_eq!(now, 1656703921);
        assert_eq!(px, 31986658);
        assert_eq!(amm.mark_std, 97995); //.068
    }

    #[test]
    fn update_mark_twap_tests() {
        let prev = 0;

        let mut now = 1;

        let mut oracle_price_data = OraclePriceData {
            price: 40_021_280 * PRICE_PRECISION_I128 / 1_000_000,
            confidence: PRICE_PRECISION / 100,
            delay: 1,
            has_sufficient_number_of_data_points: true,
        };

        // $40 everything init
        let mut amm = AMM {
            quote_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            base_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            peg_multiplier: 40 * PEG_PRECISION,
            base_spread: 0,
            long_spread: 0,
            short_spread: 0,
            last_mark_price_twap: (40 * PRICE_PRECISION),
            last_bid_price_twap: (40 * PRICE_PRECISION),
            last_ask_price_twap: (40 * PRICE_PRECISION),
            last_mark_price_twap_ts: prev,
            funding_period: 3600,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price: (40 * PRICE_PRECISION) as i128,
                last_oracle_price_twap: (40 * PRICE_PRECISION) as i128,
                last_oracle_price_twap_ts: prev,
                ..HistoricalOracleData::default()
            },
            ..AMM::default()
        };

        update_oracle_price_twap(&mut amm, now, &oracle_price_data, None).unwrap();
        assert_eq!(
            amm.historical_oracle_data.last_oracle_price,
            oracle_price_data.price
        );
        assert_eq!(
            amm.historical_oracle_data.last_oracle_price,
            40_021_280 * PRICE_PRECISION_I128 / 1_000_000
        );

        let trade_price = 40_051_280 * PRICE_PRECISION / 1_000_000;
        let trade_direction = PositionDirection::Long;

        let old_mark_twap = amm.last_mark_price_twap;
        let new_mark_twap =
            update_mark_twap(&mut amm, now, Some(trade_price), Some(trade_direction)).unwrap();
        let new_bid_twap = amm.last_bid_price_twap;
        let new_ask_twap = amm.last_ask_price_twap;

        assert!(new_mark_twap > old_mark_twap);
        assert_eq!(new_ask_twap, 40000015);
        assert_eq!(new_bid_twap, 40000006);
        assert_eq!(new_mark_twap, 40000010);
        assert!(new_bid_twap < new_ask_twap);

        while now < 3600 {
            now += 1;
            update_oracle_price_twap(&mut amm, now, &oracle_price_data, None).unwrap();
            update_mark_twap(&mut amm, now, Some(trade_price), Some(trade_direction)).unwrap();
        }

        let new_oracle_twap = amm.historical_oracle_data.last_oracle_price_twap;
        let new_mark_twap = amm.last_mark_price_twap;
        let new_bid_twap = amm.last_bid_price_twap;
        let new_ask_twap = amm.last_ask_price_twap;

        assert!(new_bid_twap < new_ask_twap);
        assert_eq!((new_bid_twap + new_ask_twap) / 2, new_mark_twap);
        assert!((new_oracle_twap as u128) < new_mark_twap); // funding in favor of maker?
        assert_eq!(new_oracle_twap, 40008161);
        assert_eq!(new_bid_twap, 40014548);
        assert_eq!(new_mark_twap, 40024054); // < 2 cents above oracle twap
        assert_eq!(new_ask_twap, 40033561);

        let trade_price_2 = 39_971_280 * PRICE_PRECISION / 1_000_000;
        let trade_direction_2 = PositionDirection::Short;
        oracle_price_data = OraclePriceData {
            price: 39_991_280 * PRICE_PRECISION_I128 / 1_000_000,
            confidence: PRICE_PRECISION / 80,
            delay: 14,
            has_sufficient_number_of_data_points: true,
        };

        while now <= 3600 * 2 {
            now += 1;
            update_oracle_price_twap(&mut amm, now, &oracle_price_data, None).unwrap();
            if now % 200 == 0 {
                update_mark_twap(&mut amm, now, Some(trade_price_2), Some(trade_direction_2))
                    .unwrap(); // ~2 cents below oracle
            }
        }

        let new_oracle_twap = amm.historical_oracle_data.last_oracle_price_twap;
        let new_mark_twap = amm.last_mark_price_twap;
        let new_bid_twap = amm.last_bid_price_twap;
        let new_ask_twap = amm.last_ask_price_twap;

        assert_eq!(new_bid_twap, 39_986_750);
        assert_eq!(new_ask_twap, 40_006_398);
        assert!(new_bid_twap < new_ask_twap);
        assert_eq!((new_bid_twap + new_ask_twap) / 2, new_mark_twap);
        // TODO fails here
        assert_eq!(new_oracle_twap, 39_998_518);
        assert_eq!(new_mark_twap, 39_996_574);
        assert_eq!(new_bid_twap, 39_986_750); // ema from prev twap
        assert_eq!(new_ask_twap, 40_006_398); // ema from prev twap

        assert!((new_oracle_twap as u128) >= new_mark_twap); // funding in favor of maker
    }

    #[test]
    fn calc_oracle_twap_tests() {
        let prev = 1656682258;
        let now = prev + 3600;

        let px = 32 * PRICE_PRECISION;

        let mut amm = AMM {
            base_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            quote_asset_reserve: 2 * AMM_RESERVE_PRECISION,
            peg_multiplier: PEG_PRECISION,
            historical_oracle_data: HistoricalOracleData {
                last_oracle_price_twap: px as i128,
                last_oracle_price_twap_ts: prev,
                ..HistoricalOracleData::default()
            },
            mark_std: PRICE_PRECISION as u64,
            last_mark_price_twap_ts: prev,
            funding_period: 3600_i64,
            ..AMM::default()
        };
        let mut oracle_price_data = OraclePriceData {
            price: (34 * PRICE_PRECISION) as i128,
            confidence: PRICE_PRECISION / 100,
            delay: 1,
            has_sufficient_number_of_data_points: true,
        };

        let _new_oracle_twap =
            update_oracle_price_twap(&mut amm, now, &oracle_price_data, None).unwrap();
        assert_eq!(
            amm.historical_oracle_data.last_oracle_price_twap,
            (34 * PRICE_PRECISION - PRICE_PRECISION / 100) as i128
        );

        // let after_ts = amm.historical_oracle_data.last_oracle_price_twap_ts;
        amm.last_mark_price_twap_ts = now - 60;
        amm.historical_oracle_data.last_oracle_price_twap_ts = now - 60;
        // let after_ts_2 = amm.historical_oracle_data.last_oracle_price_twap_ts;
        oracle_price_data = OraclePriceData {
            price: (31 * PRICE_PRECISION) as i128,
            confidence: 0,
            delay: 2,
            has_sufficient_number_of_data_points: true,
        };
        // let old_oracle_twap_2 = amm.historical_oracle_data.last_oracle_price_twap;
        let _new_oracle_twap_2 =
            update_oracle_price_twap(&mut amm, now, &oracle_price_data, None).unwrap();
        assert_eq!(amm.historical_oracle_data.last_oracle_price_twap, 33940167);
        assert_eq!(
            amm.historical_oracle_data.last_oracle_price_twap_5min,
            33392001
        );

        let _new_oracle_twap_2 =
            update_oracle_price_twap(&mut amm, now + 60 * 5, &oracle_price_data, None).unwrap();

        assert_eq!(amm.historical_oracle_data.last_oracle_price_twap, 33695154);
        assert_eq!(
            amm.historical_oracle_data.last_oracle_price_twap_5min,
            31 * PRICE_PRECISION_I128
        );

        oracle_price_data = OraclePriceData {
            price: (32 * PRICE_PRECISION) as i128,
            confidence: 0,
            delay: 2,
            has_sufficient_number_of_data_points: true,
        };

        let _new_oracle_twap_2 =
            update_oracle_price_twap(&mut amm, now + 60 * 5 + 60, &oracle_price_data, None)
                .unwrap();
        assert_eq!(
            amm.historical_oracle_data.last_oracle_price_twap_5min,
            31200001
        );
    }

    #[test]
    fn calculate_k_tests_with_spread() {
        let mut market = PerpMarket {
            amm: AMM {
                base_asset_reserve: 512295081967,
                quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
                concentration_coef: MAX_CONCENTRATION_COEFFICIENT,
                sqrt_k: 500 * AMM_RESERVE_PRECISION,
                peg_multiplier: 50000000,
                net_base_asset_amount: -12295081967,
                ..AMM::default()
            },
            ..PerpMarket::default()
        };
        market.amm.max_base_asset_reserve = u128::MAX;
        market.amm.min_base_asset_reserve = 0;
        market.amm.base_spread = 10;
        market.amm.long_spread = 5;
        market.amm.short_spread = 5;

        let (new_ask_base_asset_reserve, new_ask_quote_asset_reserve) =
            crate::amm::calculate_spread_reserves(&market.amm, PositionDirection::Long).unwrap();
        let (new_bid_base_asset_reserve, new_bid_quote_asset_reserve) =
            crate::amm::calculate_spread_reserves(&market.amm, PositionDirection::Short).unwrap();

        market.amm.ask_base_asset_reserve = new_ask_base_asset_reserve;
        market.amm.bid_base_asset_reserve = new_bid_base_asset_reserve;
        market.amm.ask_quote_asset_reserve = new_ask_quote_asset_reserve;
        market.amm.bid_quote_asset_reserve = new_bid_quote_asset_reserve;

        validate!(
            market.amm.bid_base_asset_reserve >= market.amm.base_asset_reserve
                && market.amm.bid_quote_asset_reserve <= market.amm.quote_asset_reserve,
            ErrorCode::DefaultError,
            "bid reserves out of wack: {} -> {}, quote: {} -> {}",
            market.amm.bid_base_asset_reserve,
            market.amm.base_asset_reserve,
            market.amm.bid_quote_asset_reserve,
            market.amm.quote_asset_reserve
        )
        .unwrap();

        // increase k by .25%
        let update_k_result =
            get_update_k_result(&market, bn::U192::from(501 * AMM_RESERVE_PRECISION), true)
                .unwrap();
        update_k(&mut market, &update_k_result).unwrap();

        validate!(
            market.amm.bid_base_asset_reserve >= market.amm.base_asset_reserve
                && market.amm.bid_quote_asset_reserve <= market.amm.quote_asset_reserve,
            ErrorCode::DefaultError,
            "bid reserves out of wack: {} -> {}, quote: {} -> {}",
            market.amm.bid_base_asset_reserve,
            market.amm.base_asset_reserve,
            market.amm.bid_quote_asset_reserve,
            market.amm.quote_asset_reserve
        )
        .unwrap();
    }

    #[test]
    fn calculate_k_tests() {
        let mut market = PerpMarket {
            amm: AMM {
                base_asset_reserve: 512295081967,
                quote_asset_reserve: 488 * AMM_RESERVE_PRECISION,
                concentration_coef: MAX_CONCENTRATION_COEFFICIENT,
                sqrt_k: 500 * AMM_RESERVE_PRECISION,
                peg_multiplier: 50000000,
                net_base_asset_amount: -12295081967,
                ..AMM::default()
            },
            ..PerpMarket::default()
        };
        // increase k by .25%
        let update_k_up =
            get_update_k_result(&market, bn::U192::from(501 * AMM_RESERVE_PRECISION), true)
                .unwrap();
        let (t_price, t_qar, t_bar) = calculate_terminal_price_and_reserves(&market.amm).unwrap();

        // new terminal reserves are balanced, terminal price = peg)
        assert_eq!(t_qar, 500 * AMM_RESERVE_PRECISION);
        assert_eq!(t_bar, 500 * AMM_RESERVE_PRECISION);
        assert_eq!(t_price, market.amm.peg_multiplier);

        assert_eq!(update_k_up.sqrt_k, 501 * AMM_RESERVE_PRECISION);
        assert_eq!(update_k_up.base_asset_reserve, 513319672130);
        assert_eq!(update_k_up.quote_asset_reserve, 488976000001);

        // cost to increase k is always positive when imbalanced
        let cost = adjust_k_cost_and_update(&mut market, &update_k_up).unwrap();
        assert_eq!(market.amm.terminal_quote_asset_reserve, 500975411043);
        assert!(cost > 0);
        assert_eq!(cost, 29448);

        let (t_price2, t_qar2, t_bar2) =
            calculate_terminal_price_and_reserves(&market.amm).unwrap();
        // since users are net short, new terminal price lower after increasing k
        assert!(t_price2 < t_price);
        // new terminal reserves are unbalanced with quote below base (lower terminal price)
        assert_eq!(t_bar2, 501024590163);
        assert_eq!(t_qar2, 500975411043);

        let curve_update_intensity = 100;
        let k_pct_upper_bound =
            K_BPS_UPDATE_SCALE + (K_BPS_INCREASE_MAX) * curve_update_intensity / 100;
        let k_pct_lower_bound =
            K_BPS_UPDATE_SCALE - (K_BPS_DECREASE_MAX) * curve_update_intensity / 100;

        // with positive budget, how much can k be increased?
        let (numer1, denom1) = _calculate_budgeted_k_scale(
            AMM_RESERVE_PRECISION * 55414,
            AMM_RESERVE_PRECISION * 55530,
            (QUOTE_PRECISION / 500) as i128, // positive budget
            36365000,
            (AMM_RESERVE_PRECISION * 66) as i128,
            k_pct_upper_bound,
            k_pct_lower_bound,
        )
        .unwrap();

        assert!(numer1 > denom1);
        assert_eq!(numer1, 8796289171560000);
        assert_eq!(denom1, 8790133110760000);

        let mut pct_change_in_k = (numer1 * 10000) / denom1;
        assert_eq!(pct_change_in_k, 10007); // k was increased .07%

        // with negative budget, how much should k be lowered?
        let (numer1, denom1) = _calculate_budgeted_k_scale(
            AMM_RESERVE_PRECISION * 55414,
            AMM_RESERVE_PRECISION * 55530,
            -((QUOTE_PRECISION / 50) as i128),
            36365000,
            (AMM_RESERVE_PRECISION * 66) as i128,
            k_pct_upper_bound,
            k_pct_lower_bound,
        )
        .unwrap();
        assert!(numer1 < denom1);
        pct_change_in_k = (numer1 * 1000000) / denom1;
        assert_eq!(pct_change_in_k, 993050); // k was decreased 0.695%

        // show non-linearity with budget
        let (numer1, denom1) = _calculate_budgeted_k_scale(
            AMM_RESERVE_PRECISION * 55414,
            AMM_RESERVE_PRECISION * 55530,
            -((QUOTE_PRECISION / 25) as i128),
            36365000,
            (AMM_RESERVE_PRECISION * 66) as i128,
            k_pct_upper_bound,
            k_pct_lower_bound,
        )
        .unwrap();
        assert!(numer1 < denom1);
        pct_change_in_k = (numer1 * 1000000) / denom1;
        assert_eq!(pct_change_in_k, 986196); // k was decreased 1.3804%

        // todo:
        let (numer1, denom1) = _calculate_budgeted_k_scale(
            500000000049750000004950,
            499999999950250000000000,
            114638,
            40000000,
            49750000004950,
            k_pct_upper_bound,
            k_pct_lower_bound,
        )
        .unwrap();

        assert!(numer1 > denom1);
        assert_eq!(numer1, 1001000);
        assert_eq!(denom1, 1000000);

        // todo:
        let (numer1, denom1) = _calculate_budgeted_k_scale(
            500000000049750000004950,
            499999999950250000000000,
            -114638,
            40000000,
            49750000004950,
            k_pct_upper_bound,
            k_pct_lower_bound,
        )
        .unwrap();

        assert!(numer1 < denom1);
        assert_eq!(numer1, 978000); // 2.2% decrease
        assert_eq!(denom1, 1000000);
    }

    #[test]
    fn calculate_k_tests_wrapper_fcn() {
        let mut market = PerpMarket {
            amm: AMM {
                base_asset_reserve: AMM_RESERVE_PRECISION * 55414,
                quote_asset_reserve: AMM_RESERVE_PRECISION * 55530,
                sqrt_k: 500 * AMM_RESERVE_PRECISION,
                peg_multiplier: 36365000,
                net_base_asset_amount: (AMM_RESERVE_PRECISION * 66) as i128,
                ..AMM::default()
            },
            ..PerpMarket::default()
        };

        let (numer1, denom1) = calculate_budgeted_k_scale(
            &mut market,
            (QUOTE_PRECISION / 500) as i128, // positive budget
            1100000,
        )
        .unwrap();

        assert_eq!(numer1, 8796289171560000);
        assert_eq!(denom1, 8790133110760000);
        assert!(numer1 > denom1);

        let pct_change_in_k = (numer1 * 10000) / denom1;
        assert_eq!(pct_change_in_k, 10007); // k was increased .07%
    }

    #[test]
    fn calculate_k_with_lps_tests() {
        let mut market = PerpMarket {
            amm: AMM {
                base_asset_reserve: 100 * AMM_RESERVE_PRECISION,
                quote_asset_reserve: 100 * AMM_RESERVE_PRECISION,
                terminal_quote_asset_reserve: 999900009999000 * AMM_RESERVE_PRECISION,
                sqrt_k: 100 * AMM_RESERVE_PRECISION,
                peg_multiplier: 50_000_000_000,
                net_base_asset_amount: (AMM_RESERVE_PRECISION / 10) as i128,
                base_asset_amount_step_size: 3,
                max_spread: 1000,
                ..AMM::default_test()
            },
            margin_ratio_initial: 1000,
            base_asset_amount_long: (AMM_RESERVE_PRECISION / 10) as i128,
            ..PerpMarket::default()
        };
        // let (t_price, _t_qar, _t_bar) = calculate_terminal_price_and_reserves(&market.amm).unwrap();
        // market.amm.terminal_quote_asset_reserve = _t_qar;

        let mut position = PerpPosition {
            ..PerpPosition::default()
        };

        mint_lp_shares(&mut position, &mut market, AMM_RESERVE_PRECISION, 0).unwrap();

        market.amm.market_position_per_lp = PerpPosition {
            base_asset_amount: 1,
            quote_asset_amount: -QUOTE_PRECISION_I128,
            ..PerpPosition::default()
        };

        let reserve_price = market.amm.reserve_price().unwrap();
        update_spreads(&mut market.amm, reserve_price).unwrap();

        settle_lp_position(&mut position, &mut market).unwrap();

        assert_eq!(position.base_asset_amount, 0);
        assert_eq!(position.quote_asset_amount, -QUOTE_PRECISION_I128);
        assert_eq!(position.last_net_base_asset_amount_per_lp, 1);
        assert_eq!(
            position.last_net_quote_asset_amount_per_lp,
            -QUOTE_PRECISION_I128
        );

        // increase k by 1%
        let update_k_up =
            get_update_k_result(&market, bn::U192::from(102 * AMM_RESERVE_PRECISION), false)
                .unwrap();
        let (t_price, _t_qar, _t_bar) = calculate_terminal_price_and_reserves(&market.amm).unwrap();

        // new terminal reserves are balanced, terminal price = peg)
        // assert_eq!(t_qar, 999900009999000);
        // assert_eq!(t_bar, 1000100000000000);
        assert_eq!(t_price, 49901136949); //
                                          // assert_eq!(update_k_up.sqrt_k, 101 * AMM_RESERVE_PRECISION);

        let cost = adjust_k_cost(&mut market, &update_k_up).unwrap();
        assert_eq!(
            market.amm.net_base_asset_amount,
            (AMM_RESERVE_PRECISION / 10) as i128
        );
        assert_eq!(cost, 49400); //0.05

        // lp whale adds
        let lp_whale_amount = 1000 * AMM_RESERVE_PRECISION;
        mint_lp_shares(&mut position, &mut market, lp_whale_amount, 0).unwrap();

        // ensure same cost
        let update_k_up =
            get_update_k_result(&market, bn::U192::from(1102 * AMM_RESERVE_PRECISION), false)
                .unwrap();
        let cost = adjust_k_cost(&mut market, &update_k_up).unwrap();
        assert_eq!(
            market.amm.net_base_asset_amount,
            (AMM_RESERVE_PRECISION / 10) as i128
        );
        assert_eq!(cost, 49450); //0.05

        let update_k_down =
            get_update_k_result(&market, bn::U192::from(1001 * AMM_RESERVE_PRECISION), false)
                .unwrap();
        let cost = adjust_k_cost(&mut market, &update_k_down).unwrap();
        assert_eq!(cost, -4995004950); //amm rug

        // lp whale removes
        burn_lp_shares(&mut position, &mut market, lp_whale_amount, 0).unwrap();

        // ensure same cost
        let update_k_up =
            get_update_k_result(&market, bn::U192::from(102 * AMM_RESERVE_PRECISION), false)
                .unwrap();
        let cost = adjust_k_cost(&mut market, &update_k_up).unwrap();
        assert_eq!(
            market.amm.net_base_asset_amount,
            (AMM_RESERVE_PRECISION / 10) as i128 - 1
        );
        assert_eq!(cost, 49450); //0.05

        let update_k_down =
            get_update_k_result(&market, bn::U192::from(79 * AMM_RESERVE_PRECISION), false)
                .unwrap();
        let cost = adjust_k_cost(&mut market, &update_k_down).unwrap();
        assert_eq!(cost, -1407000); //0.05

        // lp owns 50% of vAMM, same k
        position.lp_shares = 50 * AMM_RESERVE_PRECISION;
        market.amm.user_lp_shares = 50 * AMM_RESERVE_PRECISION;
        // cost to increase k is always positive when imbalanced
        let cost = adjust_k_cost(&mut market, &update_k_up).unwrap();
        assert_eq!(
            market.amm.net_base_asset_amount,
            (AMM_RESERVE_PRECISION / 10) as i128 - 1
        );
        assert_eq!(cost, 187800); //0.19

        // lp owns 99% of vAMM, same k
        position.lp_shares = 99 * AMM_RESERVE_PRECISION;
        market.amm.user_lp_shares = 99 * AMM_RESERVE_PRECISION;
        let cost2 = adjust_k_cost(&mut market, &update_k_up).unwrap();
        assert!(cost2 > cost);
        assert_eq!(cost2, 76804900); //216.45

        // lp owns 100% of vAMM, same k
        position.lp_shares = 100 * AMM_RESERVE_PRECISION;
        market.amm.user_lp_shares = 100 * AMM_RESERVE_PRECISION;
        let cost3 = adjust_k_cost(&mut market, &update_k_up).unwrap();
        assert!(cost3 > cost);
        assert!(cost3 > cost2);
        assert_eq!(cost3, 216450200);

        // //  todo: support this
        // market.amm.net_base_asset_amount = -(AMM_RESERVE_PRECISION as i128);
        // let cost2 = adjust_k_cost(&mut market, &update_k_up).unwrap();
        // assert!(cost2 > cost);
        // assert_eq!(cost2, 249999999999850000000001);
    }
}
