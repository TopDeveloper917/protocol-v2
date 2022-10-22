use solana_program::msg;

use crate::error::{ClearingHouseResult, ErrorCode};
use crate::math::casting::Cast;
use crate::math::helpers::{get_proportion_u128, log10_iter};
use crate::math::safe_math::SafeMath;

use crate::state::insurance_fund_stake::InsuranceFundStake;
use crate::state::spot_market::SpotMarket;
use crate::validate;

#[cfg(test)]
mod tests;

pub fn vault_amount_to_if_shares(
    amount: u64,
    total_if_shares: u128,
    insurance_fund_vault_balance: u64,
) -> ClearingHouseResult<u128> {
    // relative to the entire pool + total amount minted
    let n_shares = if insurance_fund_vault_balance > 0 {
        // assumes total_if_shares != 0 (in most cases) for nice result for user

        get_proportion_u128(
            amount.cast::<u128>()?,
            total_if_shares,
            insurance_fund_vault_balance.cast::<u128>()?,
        )?
    } else {
        // must be case that total_if_shares == 0 for nice result for user
        validate!(
            total_if_shares == 0,
            ErrorCode::DefaultError,
            "assumes total_if_shares == 0",
        )?;

        amount.cast::<u128>()?
    };

    Ok(n_shares)
}

pub fn if_shares_to_vault_amount(
    n_shares: u128,
    total_if_shares: u128,
    insurance_fund_vault_balance: u64,
) -> ClearingHouseResult<u64> {
    validate!(
        n_shares <= total_if_shares,
        ErrorCode::DefaultError,
        "n_shares({}) > total_if_shares({})",
        n_shares,
        total_if_shares
    )?;

    let amount = if total_if_shares > 0 {
        get_proportion_u128(
            insurance_fund_vault_balance as u128,
            n_shares,
            total_if_shares as u128,
        )?
        .cast::<u64>()?
    } else {
        0
    };

    Ok(amount)
}

pub fn calculate_rebase_info(
    total_if_shares: u128,
    insurance_fund_vault_balance: u64,
) -> ClearingHouseResult<(u32, u128)> {
    let rebase_divisor_full = total_if_shares
        .safe_div(10)?
        .safe_div(insurance_fund_vault_balance.cast::<u128>()?)?;

    let expo_diff = log10_iter(rebase_divisor_full).cast::<u32>()?;
    let rebase_divisor = 10_u128.pow(expo_diff);

    Ok((expo_diff, rebase_divisor))
}

pub fn calculate_if_shares_lost(
    insurance_fund_stake: &InsuranceFundStake,
    spot_market: &SpotMarket,
    insurance_fund_vault_balance: u64,
) -> ClearingHouseResult<u128> {
    let n_shares = insurance_fund_stake.last_withdraw_request_shares;

    let amount = if_shares_to_vault_amount(
        n_shares,
        spot_market.insurance_fund.total_shares,
        insurance_fund_vault_balance,
    )?;

    let if_shares_lost = if amount > insurance_fund_stake.last_withdraw_request_value {
        let new_n_shares = vault_amount_to_if_shares(
            insurance_fund_stake.last_withdraw_request_value,
            spot_market.insurance_fund.total_shares - n_shares,
            insurance_fund_vault_balance - insurance_fund_stake.last_withdraw_request_value,
        )?;

        validate!(
            new_n_shares <= n_shares,
            ErrorCode::DefaultError,
            "Issue calculating delta if_shares after canceling request {} < {}",
            new_n_shares,
            n_shares
        )?;

        n_shares.safe_sub(new_n_shares)?
    } else {
        0
    };

    Ok(if_shares_lost)
}
