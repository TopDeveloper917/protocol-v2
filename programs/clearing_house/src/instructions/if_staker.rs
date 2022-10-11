use anchor_lang::prelude::*;
use anchor_spl::token::{Token, TokenAccount};

use crate::error::ErrorCode;
use crate::instructions::constraints::*;
use crate::load_mut;
use crate::state::insurance_fund_stake::InsuranceFundStake;
use crate::state::spot_market::SpotMarket;
use crate::state::state::State;
use crate::state::user::UserStats;
use crate::validate;
use crate::{controller, math};

pub fn handle_initialize_insurance_fund_stake(
    ctx: Context<InitializeInsuranceFundStake>,
    market_index: u16,
) -> Result<()> {
    let mut if_stake = ctx
        .accounts
        .insurance_fund_stake
        .load_init()
        .or(Err(ErrorCode::UnableToLoadAccountLoader))?;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    *if_stake = InsuranceFundStake::new(*ctx.accounts.authority.key, market_index, now);

    Ok(())
}

pub fn handle_add_insurance_fund_stake(
    ctx: Context<AddInsuranceFundStake>,
    market_index: u16,
    amount: u64,
) -> Result<()> {
    if amount == 0 {
        return Err(ErrorCode::InsufficientDeposit.into());
    }

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
    let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
    let spot_market = &mut load_mut!(ctx.accounts.spot_market)?;
    let state = &ctx.accounts.state;

    validate!(
        insurance_fund_stake.market_index == market_index,
        ErrorCode::DefaultError,
        "insurance_fund_stake does not match market_index"
    )?;

    validate!(
        insurance_fund_stake.last_withdraw_request_shares == 0
            && insurance_fund_stake.last_withdraw_request_value == 0,
        ErrorCode::DefaultError,
        "withdraw request in progress"
    )?;

    {
        controller::insurance::attempt_settle_revenue_to_insurance_fund(
            &ctx.accounts.spot_market_vault,
            &ctx.accounts.insurance_fund_vault,
            spot_market,
            now,
            &ctx.accounts.token_program,
            &ctx.accounts.clearing_house_signer,
            state,
        )?;
    }

    controller::insurance::add_insurance_fund_stake(
        amount,
        ctx.accounts.insurance_fund_vault.amount,
        insurance_fund_stake,
        user_stats,
        spot_market,
        clock.unix_timestamp,
    )?;

    controller::token::receive(
        &ctx.accounts.token_program,
        &ctx.accounts.user_token_account,
        &ctx.accounts.insurance_fund_vault,
        &ctx.accounts.authority,
        amount,
    )?;

    Ok(())
}

pub fn handle_request_remove_insurance_fund_stake(
    ctx: Context<RequestRemoveInsuranceFundStake>,
    market_index: u16,
    amount: u64,
) -> Result<()> {
    let clock = Clock::get()?;
    let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
    let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
    let spot_market = &mut load_mut!(ctx.accounts.spot_market)?;

    validate!(
        insurance_fund_stake.market_index == market_index,
        ErrorCode::DefaultError,
        "insurance_fund_stake does not match market_index"
    )?;

    validate!(
        insurance_fund_stake.last_withdraw_request_shares == 0,
        ErrorCode::DefaultError,
        "Withdraw request is already in progress"
    )?;

    let n_shares = math::insurance::vault_amount_to_if_shares(
        amount,
        spot_market.insurance_fund.total_shares,
        ctx.accounts.insurance_fund_vault.amount,
    )?;

    validate!(
        n_shares > 0,
        ErrorCode::DefaultError,
        "Requested lp_shares = 0"
    )?;

    let user_if_shares = insurance_fund_stake.checked_if_shares(spot_market)?;
    validate!(user_if_shares >= n_shares, ErrorCode::InsufficientIFShares)?;

    controller::insurance::request_remove_insurance_fund_stake(
        n_shares,
        ctx.accounts.insurance_fund_vault.amount,
        insurance_fund_stake,
        user_stats,
        spot_market,
        clock.unix_timestamp,
    )?;

    Ok(())
}

pub fn handle_cancel_request_remove_insurance_fund_stake(
    ctx: Context<RequestRemoveInsuranceFundStake>,
    market_index: u16,
) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
    let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
    let spot_market = &mut load_mut!(ctx.accounts.spot_market)?;

    validate!(
        insurance_fund_stake.market_index == market_index,
        ErrorCode::DefaultError,
        "insurance_fund_stake does not match market_index"
    )?;

    validate!(
        insurance_fund_stake.last_withdraw_request_shares != 0,
        ErrorCode::DefaultError,
        "No withdraw request in progress"
    )?;

    controller::insurance::cancel_request_remove_insurance_fund_stake(
        ctx.accounts.insurance_fund_vault.amount,
        insurance_fund_stake,
        user_stats,
        spot_market,
        now,
    )?;

    Ok(())
}

#[access_control(
    withdraw_not_paused(&ctx.accounts.state)
)]
pub fn handle_remove_insurance_fund_stake(
    ctx: Context<RemoveInsuranceFundStake>,
    market_index: u16,
) -> Result<()> {
    let clock = Clock::get()?;
    let now = clock.unix_timestamp;
    let insurance_fund_stake = &mut load_mut!(ctx.accounts.insurance_fund_stake)?;
    let user_stats = &mut load_mut!(ctx.accounts.user_stats)?;
    let spot_market = &mut load_mut!(ctx.accounts.spot_market)?;
    let state = &ctx.accounts.state;

    validate!(
        insurance_fund_stake.market_index == market_index,
        ErrorCode::DefaultError,
        "insurance_fund_stake does not match market_index"
    )?;

    let amount = controller::insurance::remove_insurance_fund_stake(
        ctx.accounts.insurance_fund_vault.amount,
        insurance_fund_stake,
        user_stats,
        spot_market,
        now,
    )?;

    controller::token::send_from_program_vault(
        &ctx.accounts.token_program,
        &ctx.accounts.insurance_fund_vault,
        &ctx.accounts.user_token_account,
        &ctx.accounts.clearing_house_signer,
        state.signer_nonce,
        amount,
    )?;

    validate!(
        ctx.accounts.insurance_fund_vault.amount > 0,
        ErrorCode::DefaultError,
        "insurance_fund_vault.amount must remain > 0"
    )?;

    Ok(())
}

#[derive(Accounts)]
#[instruction(
    market_index: u16,
)]
pub struct InitializeInsuranceFundStake<'info> {
    #[account(
        seeds = [b"spot_market", market_index.to_le_bytes().as_ref()],
        bump
    )]
    pub spot_market: AccountLoader<'info, SpotMarket>,
    #[account(
        init,
        seeds = [b"insurance_fund_stake", authority.key.as_ref(), market_index.to_le_bytes().as_ref()],
        space = std::mem::size_of::<InsuranceFundStake>() + 8,
        bump,
        payer = payer
    )]
    pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
    #[account(
        mut,
        has_one = authority
    )]
    pub user_stats: AccountLoader<'info, UserStats>,
    pub state: Box<Account<'info, State>>,
    pub authority: Signer<'info>,
    #[account(mut)]
    pub payer: Signer<'info>,
    pub rent: Sysvar<'info, Rent>,
    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
#[instruction(market_index: u16)]
pub struct AddInsuranceFundStake<'info> {
    pub state: Box<Account<'info, State>>,
    #[account(
        seeds = [b"spot_market", market_index.to_le_bytes().as_ref()],
        bump
    )]
    pub spot_market: AccountLoader<'info, SpotMarket>,
    #[account(
        mut,
        has_one = authority,
    )]
    pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
    #[account(
        mut,
        has_one = authority,
    )]
    pub user_stats: AccountLoader<'info, UserStats>,
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"spot_market_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
    pub spot_market_vault: Box<Account<'info, TokenAccount>>,
    #[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
    pub insurance_fund_vault: Box<Account<'info, TokenAccount>>,

    #[account(
        constraint = state.signer.eq(&clearing_house_signer.key())
    )]
    /// CHECK: forced clearing_house_signer
    pub clearing_house_signer: AccountInfo<'info>,
    #[account(
        mut,
        token::mint = insurance_fund_vault.mint,
        token::authority = authority
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct RequestRemoveInsuranceFundStake<'info> {
    #[account(
        seeds = [b"spot_market", market_index.to_le_bytes().as_ref()],
        bump
    )]
    pub spot_market: AccountLoader<'info, SpotMarket>,
    #[account(
        mut,
        has_one = authority,
    )]
    pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
    #[account(
        mut,
        has_one = authority,
    )]
    pub user_stats: AccountLoader<'info, UserStats>,
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
    pub insurance_fund_vault: Box<Account<'info, TokenAccount>>,
}

#[derive(Accounts)]
#[instruction(market_index: u16,)]
pub struct RemoveInsuranceFundStake<'info> {
    pub state: Box<Account<'info, State>>,
    #[account(
        seeds = [b"spot_market", market_index.to_le_bytes().as_ref()],
        bump
    )]
    pub spot_market: AccountLoader<'info, SpotMarket>,
    #[account(
        mut,
        has_one = authority,
    )]
    pub insurance_fund_stake: AccountLoader<'info, InsuranceFundStake>,
    #[account(
        mut,
        has_one = authority,
    )]
    pub user_stats: AccountLoader<'info, UserStats>,
    pub authority: Signer<'info>,
    #[account(
        mut,
        seeds = [b"insurance_fund_vault".as_ref(), market_index.to_le_bytes().as_ref()],
        bump,
    )]
    pub insurance_fund_vault: Box<Account<'info, TokenAccount>>,
    #[account(
        constraint = state.signer.eq(&clearing_house_signer.key())
    )]
    /// CHECK: forced clearing_house_signer
    pub clearing_house_signer: AccountInfo<'info>,
    #[account(
        mut,
        token::mint = insurance_fund_vault.mint,
        token::authority = authority
    )]
    pub user_token_account: Box<Account<'info, TokenAccount>>,
    pub token_program: Program<'info, Token>,
}
