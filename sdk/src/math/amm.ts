import { BN } from '@project-serum/anchor';
import {
	AMM_MANTISSA,
	ONE,
	PEG_SCALAR,
	ZERO,
} from '../constants/numericConstants';
import { AMM, PositionDirection, SwapDirection } from '../types';
import { assert } from '../assert/assert';

export function calculateCurvePriceWithMantissa(
	baseAssetAmount: BN,
	quoteAssetAmount: BN,
	peg: BN
): BN {
	if (baseAssetAmount.abs().lte(ZERO)) {
		return new BN(0);
	}

	return quoteAssetAmount
		.mul(AMM_MANTISSA)
		.mul(peg)
		.div(PEG_SCALAR)
		.div(baseAssetAmount);
}

export type AssetType = 'quote' | 'base';

/**
 * Calculates what the amm reserves would be after swapping a quote or base asset amount.
 *
 * @param amm
 * @param inputAssetType
 * @param swapAmount
 * @param swapDirection
 * @returns quoteAssetReserve and baseAssetReserve after swap. precision: 10^13
 */
export function calculateAmmReservesAfterSwap(
	amm: AMM,
	inputAssetType: AssetType,
	swapAmount: BN,
	swapDirection: SwapDirection
): [BN, BN] {
	assert(swapAmount.gte(ZERO), 'swapAmount must be greater than 0');

	let newQuoteAssetReserve;
	let newBaseAssetReserve;

	if (inputAssetType === 'quote') {
		const swapAmountIntermediate = swapAmount.mul(AMM_MANTISSA);
		swapAmount = swapAmountIntermediate.div(amm.pegMultiplier);

		// Because ints round down by default, we need to add 1 back when removing from
		// AMM to avoid giving users extra pnl when they short
		const roundUp =
			swapDirection === SwapDirection.REMOVE &&
			!swapAmountIntermediate.mod(amm.pegMultiplier).eq(ZERO);
		if (roundUp) {
			swapAmount = swapAmount.add(ONE);
		}

		[newQuoteAssetReserve, newBaseAssetReserve] = calculateSwapOutput(
			amm.quoteAssetReserve,
			swapAmount,
			swapDirection,
			amm.sqrtK.mul(amm.sqrtK)
		);
	} else {
		[newBaseAssetReserve, newQuoteAssetReserve] = calculateSwapOutput(
			amm.baseAssetReserve,
			swapAmount,
			swapDirection,
			amm.sqrtK.mul(amm.sqrtK)
		);
	}

	return [newQuoteAssetReserve, newBaseAssetReserve];
}

/**
 * Helper function calculating constant product curve output. Agnostic to whether input asset is quote or base
 *
 * @param inputAssetReserve
 * @param swapAmount
 * @param swapDirection
 * @param invariant
 * @returns newInputAssetReserve and newOutputAssetReserve after swap. precision: 10^13
 */
export function calculateSwapOutput(
	inputAssetReserve: BN,
	swapAmount: BN,
	swapDirection: SwapDirection,
	invariant: BN
): [BN, BN] {
	let newInputAssetReserve;
	if (swapDirection === SwapDirection.ADD) {
		newInputAssetReserve = inputAssetReserve.add(swapAmount);
	} else {
		newInputAssetReserve = inputAssetReserve.sub(swapAmount);
	}
	const newOutputAssetReserve = invariant.div(newInputAssetReserve);
	return [newInputAssetReserve, newOutputAssetReserve];
}

/**
 * Translate long/shorting quote/base asset into amm operation
 *
 * @param inputAssetType
 * @param positionDirection
 */
export function getSwapDirection(
	inputAssetType: AssetType,
	positionDirection: PositionDirection
): SwapDirection {
	if (
		positionDirection === PositionDirection.LONG &&
		inputAssetType === 'base'
	) {
		return SwapDirection.REMOVE;
	}

	if (
		positionDirection === PositionDirection.SHORT &&
		inputAssetType === 'quote'
	) {
		return SwapDirection.REMOVE;
	}

	return SwapDirection.ADD;
}
