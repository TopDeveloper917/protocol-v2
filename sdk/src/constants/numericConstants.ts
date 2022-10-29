import { LAMPORTS_PER_SOL } from '@solana/web3.js';
import { BN } from '../';

export const ZERO = new BN(0);
export const ONE = new BN(1);
export const TWO = new BN(2);
export const THREE = new BN(3);
export const FOUR = new BN(4);
export const FIVE = new BN(5);
export const SIX = new BN(6);
export const SEVEN = new BN(7);
export const EIGHT = new BN(8);
export const NINE = new BN(9);
export const TEN = new BN(10);
export const TEN_THOUSAND = new BN(10000);
export const BN_MAX = new BN(Number.MAX_SAFE_INTEGER);
export const TEN_MILLION = TEN_THOUSAND.mul(TEN_THOUSAND);

export const MAX_LEVERAGE = new BN(5);

export const PERCENTAGE_PRECISION_EXP = new BN(6);
export const PERCENTAGE_PRECISION = new BN(10).pow(PERCENTAGE_PRECISION_EXP);
export const CONCENTRATION_PRECISION = PERCENTAGE_PRECISION;

export const QUOTE_PRECISION_EXP = new BN(6);
export const FUNDING_RATE_BUFFER_PRECISION_EXP = new BN(3);
export const PRICE_PRECISION_EXP = new BN(6);
export const FUNDING_RATE_PRECISION_EXP = PRICE_PRECISION_EXP.add(
	FUNDING_RATE_BUFFER_PRECISION_EXP
);
export const PEG_PRECISION_EXP = new BN(6);
export const AMM_RESERVE_PRECISION_EXP = new BN(9);

export const SPOT_MARKET_RATE_PRECISION_EXP = new BN(6);
export const SPOT_MARKET_RATE_PRECISION = new BN(10).pow(
	SPOT_MARKET_RATE_PRECISION_EXP
);

export const SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION_EXP = new BN(10);
export const SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION = new BN(10).pow(
	SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION_EXP
);

export const SPOT_MARKET_UTILIZATION_PRECISION_EXP = new BN(6);
export const SPOT_MARKET_UTILIZATION_PRECISION = new BN(10).pow(
	SPOT_MARKET_UTILIZATION_PRECISION_EXP
);

export const SPOT_MARKET_WEIGHT_PRECISION = new BN(10000);
export const SPOT_MARKET_BALANCE_PRECISION_EXP = new BN(9);
export const SPOT_MARKET_BALANCE_PRECISION = new BN(10).pow(
	SPOT_MARKET_BALANCE_PRECISION_EXP
);
export const SPOT_MARKET_IMF_PRECISION_EXP = new BN(6);

export const SPOT_MARKET_IMF_PRECISION = new BN(10).pow(
	SPOT_MARKET_IMF_PRECISION_EXP
);
export const LIQUIDATION_FEE_PRECISION = new BN(1000000);

export const QUOTE_PRECISION = new BN(10).pow(QUOTE_PRECISION_EXP);
export const PRICE_PRECISION = new BN(10).pow(PRICE_PRECISION_EXP);
export const FUNDING_RATE_PRECISION = new BN(10).pow(
	FUNDING_RATE_PRECISION_EXP
);
export const FUNDING_RATE_BUFFER_PRECISION = new BN(10).pow(
	FUNDING_RATE_BUFFER_PRECISION_EXP
);
export const PEG_PRECISION = new BN(10).pow(PEG_PRECISION_EXP);

export const AMM_RESERVE_PRECISION = new BN(10).pow(AMM_RESERVE_PRECISION_EXP);

export const BASE_PRECISION = AMM_RESERVE_PRECISION;
export const BASE_PRECISION_EXP = AMM_RESERVE_PRECISION_EXP;

export const AMM_TO_QUOTE_PRECISION_RATIO =
	AMM_RESERVE_PRECISION.div(QUOTE_PRECISION); // 10^3
export const PRICE_DIV_PEG = PRICE_PRECISION.div(PEG_PRECISION); //10^1
export const PRICE_TO_QUOTE_PRECISION = PRICE_PRECISION.div(QUOTE_PRECISION); // 10^1
export const AMM_TIMES_PEG_TO_QUOTE_PRECISION_RATIO =
	AMM_RESERVE_PRECISION.mul(PEG_PRECISION).div(QUOTE_PRECISION); // 10^9
export const MARGIN_PRECISION = TEN_THOUSAND;
export const BID_ASK_SPREAD_PRECISION = new BN(1000000); // 10^6

export const ONE_YEAR = new BN(31536000);

export const QUOTE_SPOT_MARKET_INDEX = 0;

export const LAMPORTS_PRECISION = new BN(LAMPORTS_PER_SOL);
export const LAMPORTS_EXP = new BN(Math.log10(LAMPORTS_PER_SOL));
