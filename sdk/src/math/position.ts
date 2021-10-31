import { Market, PositionDirection, UserPosition } from '../types';
import { ZERO } from '../constants/numericConstants';
import BN from 'bn.js';
import { findSwapOutput } from './amm';
import {
	AMM_MANTISSA,
	BASE_ASSET_PRECISION,
	FUNDING_MANTISSA,
	PRICE_TO_USDC_PRECISION,
} from '../clearingHouse';

/**
 * calculateBaseAssetValue
 * = market value of closing entire position
 * @param market
 * @param userPosition
 * @returns precision = 1e10 (AMM_MANTISSA)
 */
export function calculateBaseAssetValue(
	market: Market,
	userPosition: UserPosition
): BN {
	if (userPosition.baseAssetAmount.eq(ZERO)) {
		return ZERO;
	}

	const directionToClose = userPosition.baseAssetAmount.gt(ZERO)
		? PositionDirection.SHORT
		: PositionDirection.LONG;

	const invariant = market.amm.sqrtK.mul(market.amm.sqrtK);
	const [, newQuoteAssetAmount] = findSwapOutput(
		market.amm.baseAssetReserve,
		market.amm.quoteAssetReserve,
		directionToClose,
		userPosition.baseAssetAmount.abs(),
		'base',
		invariant,
		market.amm.pegMultiplier
	);

	switch (directionToClose) {
		case PositionDirection.SHORT:
			return market.amm.quoteAssetReserve
				.sub(newQuoteAssetAmount)
				.mul(market.amm.pegMultiplier);

		case PositionDirection.LONG:
			return newQuoteAssetAmount
				.sub(market.amm.quoteAssetReserve)
				.mul(market.amm.pegMultiplier);
	}
}

/**
 * calculatePositionPNL
 * = BaseAssetAmount * (Avg Exit Price - Avg Entry Price)
 * @param market
 * @param marketPosition
 * @param withFunding (adds unrealized funding payment pnl to result)
 * @returns precision = 1e6 (USDC_PRECISION)
 */
export function calculatePositionPNL(
	market: Market,
	marketPosition: UserPosition,
	withFunding = false
): BN {
	if (marketPosition.baseAssetAmount.eq(ZERO)) {
		return ZERO;
	}

	const directionToClose = marketPosition.baseAssetAmount.gt(ZERO)
		? PositionDirection.SHORT
		: PositionDirection.LONG;

	const baseAssetValue = calculateBaseAssetValue(market, marketPosition).div(
		AMM_MANTISSA
	);
	let pnlAssetAmount;

	switch (directionToClose) {
		case PositionDirection.SHORT:
			pnlAssetAmount = baseAssetValue.sub(marketPosition.quoteAssetAmount);
			break;

		case PositionDirection.LONG:
			pnlAssetAmount = marketPosition.quoteAssetAmount.sub(baseAssetValue);
			break;
	}

	if (withFunding) {
		const fundingRatePnL = calculatePositionFundingPNL(
			market,
			marketPosition
		).div(PRICE_TO_USDC_PRECISION);

		pnlAssetAmount = pnlAssetAmount.add(fundingRatePnL);
	}

	return pnlAssetAmount;
}

export function calculatePositionFundingPNL(
	market: Market,
	marketPosition: UserPosition
): BN {
	if (marketPosition.baseAssetAmount.eq(ZERO)) {
		return ZERO;
	}

	let ammCumulativeFundingRate: BN;
	if (marketPosition.baseAssetAmount.gt(ZERO)) {
		ammCumulativeFundingRate = market.amm.cumulativeFundingRateLong;
	} else {
		ammCumulativeFundingRate = market.amm.cumulativeFundingRateShort;
	}

	const perPositionFundingRate = ammCumulativeFundingRate
		.sub(marketPosition.lastCumulativeFundingRate)
		.mul(marketPosition.baseAssetAmount)
		.div(BASE_ASSET_PRECISION)
		.div(FUNDING_MANTISSA)
		.mul(new BN(-1));

	return perPositionFundingRate;
}
