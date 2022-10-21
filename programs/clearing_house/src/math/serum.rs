use crate::error::ClearingHouseResult;
use crate::math::casting::{cast, cast_to_u128};
use crate::math::constants::PRICE_TO_QUOTE_PRECISION_RATIO;
use crate::math::safe_math::SafeMath;

// Max amount of base to put deposit into serum
pub fn calculate_serum_max_coin_qty(
    base_asset_amount: u64,
    coin_lot_size: u64,
) -> ClearingHouseResult<u64> {
    base_asset_amount.safe_div(coin_lot_size)
}

// calculate limit price in serum lot sizes
pub fn calculate_serum_limit_price(
    limit_price: u128,
    pc_lot_size: u64,
    coin_decimals: u32,
    coin_lot_size: u64,
) -> ClearingHouseResult<u64> {
    let coin_precision = 10_u128.pow(coin_decimals);

    limit_price
        .safe_div(PRICE_TO_QUOTE_PRECISION_RATIO)?
        .safe_mul(cast(coin_lot_size)?)?
        .safe_div(cast_to_u128(pc_lot_size)?.safe_mul(coin_precision)?)
        .map(|limit_price| limit_price as u64)
}

// Max amount of quote to put deposit into serum
pub fn calculate_serum_max_native_pc_quantity(
    serum_limit_price: u64,
    serum_coin_qty: u64,
    pc_lot_size: u64,
) -> ClearingHouseResult<u64> {
    pc_lot_size
        .safe_add(pc_lot_size / 2500)? // max 4bps
        .safe_mul(serum_limit_price)?
        .safe_mul(serum_coin_qty)?
        .safe_mul(10004)?
        .safe_div(10000)
}

pub fn calculate_price_from_serum_limit_price(
    limit_price: u64,
    pc_lot_size: u64,
    coin_decimals: u32,
    coin_lot_size: u64,
) -> ClearingHouseResult<u128> {
    let coin_precision = 10_u128.pow(coin_decimals);

    cast_to_u128(limit_price)?
        .safe_mul(cast_to_u128(pc_lot_size)?.safe_mul(coin_precision)?)?
        .safe_mul(PRICE_TO_QUOTE_PRECISION_RATIO)?
        .safe_div(cast(coin_lot_size)?)
}
