use crate::error::{ClearingHouseResult, ErrorCode};
use crate::math::constants::SPOT_UTILIZATION_PRECISION_U32;
use crate::validate;
use solana_program::msg;

pub fn validate_borrow_rate(
    optimal_utilization: u32,
    optimal_borrow_rate: u32,
    max_borrow_rate: u32,
) -> ClearingHouseResult {
    validate!(
        optimal_utilization <= SPOT_UTILIZATION_PRECISION_U32,
        ErrorCode::InvalidSpotMarketInitialization,
        "For spot market, optimal_utilization must be < {}",
        SPOT_UTILIZATION_PRECISION_U32
    )?;

    validate!(
        optimal_borrow_rate <= max_borrow_rate,
        ErrorCode::InvalidSpotMarketInitialization,
        "For spot market, optimal borrow rate ({}) must be <  max borrow rate ({})",
        optimal_borrow_rate,
        max_borrow_rate
    )?;

    Ok(())
}
