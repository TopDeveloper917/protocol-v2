import { MarketType, PerpMarketAccount, PositionDirection } from '../types';
import { BN } from '@project-serum/anchor';
import { assert } from '../assert/assert';
import {
	PRICE_PRECISION,
	PEG_PRECISION,
	AMM_TO_QUOTE_PRECISION_RATIO,
	ZERO,
	BASE_PRECISION,
} from '../constants/numericConstants';
import {
	calculateBidPrice,
	calculateAskPrice,
	calculateReservePrice,
} from './market';
import {
	calculateAmmReservesAfterSwap,
	calculatePrice,
	getSwapDirection,
	AssetType,
	calculateUpdatedAMMSpreadReserves,
	calculateQuoteAssetAmountSwapped,
} from './amm';
import { squareRootBN } from './utils';
import { isVariant } from '../types';
import { OraclePriceData } from '../oracles/types';
import { DLOB } from '../dlob/DLOB';
import { PublicKey } from '@solana/web3.js';

const MAXPCT = new BN(1000); //percentage units are [0,1000] => [0,1]

export type PriceImpactUnit =
	| 'entryPrice'
	| 'maxPrice'
	| 'priceDelta'
	| 'priceDeltaAsNumber'
	| 'pctAvg'
	| 'pctMax'
	| 'quoteAssetAmount'
	| 'quoteAssetAmountPeg'
	| 'acquiredBaseAssetAmount'
	| 'acquiredQuoteAssetAmount'
	| 'all';

/**
 * Calculates avg/max slippage (price impact) for candidate trade
 * @param direction
 * @param amount
 * @param market
 * @param inputAssetType which asset is being traded
 * @param useSpread whether to consider spread with calculating slippage
 * @return [pctAvgSlippage, pctMaxSlippage, entryPrice, newPrice]
 *
 * 'pctAvgSlippage' =>  the percentage change to entryPrice (average est slippage in execution) : Precision PRICE_PRECISION
 *
 * 'pctMaxSlippage' =>  the percentage change to maxPrice (highest est slippage in execution) : Precision PRICE_PRECISION
 *
 * 'entryPrice' => the average price of the trade : Precision PRICE_PRECISION
 *
 * 'newPrice' => the price of the asset after the trade : Precision PRICE_PRECISION
 */
export function calculateTradeSlippage(
	direction: PositionDirection,
	amount: BN,
	market: PerpMarketAccount,
	inputAssetType: AssetType = 'quote',
	oraclePriceData: OraclePriceData,
	useSpread = true
): [BN, BN, BN, BN] {
	let oldPrice: BN;

	if (useSpread && market.amm.baseSpread > 0) {
		if (isVariant(direction, 'long')) {
			oldPrice = calculateAskPrice(market, oraclePriceData);
		} else {
			oldPrice = calculateBidPrice(market, oraclePriceData);
		}
	} else {
		oldPrice = calculateReservePrice(market, oraclePriceData);
	}
	if (amount.eq(ZERO)) {
		return [ZERO, ZERO, oldPrice, oldPrice];
	}
	const [acquiredBaseReserve, acquiredQuoteReserve, acquiredQuoteAssetAmount] =
		calculateTradeAcquiredAmounts(
			direction,
			amount,
			market,
			inputAssetType,
			oraclePriceData,
			useSpread
		);

	const entryPrice = acquiredQuoteAssetAmount
		.mul(AMM_TO_QUOTE_PRECISION_RATIO)
		.mul(PRICE_PRECISION)
		.div(acquiredBaseReserve.abs());

	let amm: Parameters<typeof calculateAmmReservesAfterSwap>[0];
	if (useSpread && market.amm.baseSpread > 0) {
		const { baseAssetReserve, quoteAssetReserve, sqrtK, newPeg } =
			calculateUpdatedAMMSpreadReserves(market.amm, direction, oraclePriceData);
		amm = {
			baseAssetReserve,
			quoteAssetReserve,
			sqrtK: sqrtK,
			pegMultiplier: newPeg,
		};
	} else {
		amm = market.amm;
	}

	const newPrice = calculatePrice(
		amm.baseAssetReserve.sub(acquiredBaseReserve),
		amm.quoteAssetReserve.sub(acquiredQuoteReserve),
		amm.pegMultiplier
	);

	if (direction == PositionDirection.SHORT) {
		assert(newPrice.lte(oldPrice));
	} else {
		assert(oldPrice.lte(newPrice));
	}

	const pctMaxSlippage = newPrice
		.sub(oldPrice)
		.mul(PRICE_PRECISION)
		.div(oldPrice)
		.abs();
	const pctAvgSlippage = entryPrice
		.sub(oldPrice)
		.mul(PRICE_PRECISION)
		.div(oldPrice)
		.abs();

	return [pctAvgSlippage, pctMaxSlippage, entryPrice, newPrice];
}

/**
 * Calculates acquired amounts for trade executed
 * @param direction
 * @param amount
 * @param market
 * @param inputAssetType
 * @param useSpread
 * @return
 * 	| 'acquiredBase' =>  positive/negative change in user's base : BN AMM_RESERVE_PRECISION
 * 	| 'acquiredQuote' => positive/negative change in user's quote : BN TODO-PRECISION
 */
export function calculateTradeAcquiredAmounts(
	direction: PositionDirection,
	amount: BN,
	market: PerpMarketAccount,
	inputAssetType: AssetType = 'quote',
	oraclePriceData: OraclePriceData,
	useSpread = true
): [BN, BN, BN] {
	if (amount.eq(ZERO)) {
		return [ZERO, ZERO, ZERO];
	}

	const swapDirection = getSwapDirection(inputAssetType, direction);

	let amm: Parameters<typeof calculateAmmReservesAfterSwap>[0];
	if (useSpread && market.amm.baseSpread > 0) {
		const { baseAssetReserve, quoteAssetReserve, sqrtK, newPeg } =
			calculateUpdatedAMMSpreadReserves(market.amm, direction, oraclePriceData);
		amm = {
			baseAssetReserve,
			quoteAssetReserve,
			sqrtK: sqrtK,
			pegMultiplier: newPeg,
		};
	} else {
		amm = market.amm;
	}

	const [newQuoteAssetReserve, newBaseAssetReserve] =
		calculateAmmReservesAfterSwap(amm, inputAssetType, amount, swapDirection);

	const acquiredBase = amm.baseAssetReserve.sub(newBaseAssetReserve);
	const acquiredQuote = amm.quoteAssetReserve.sub(newQuoteAssetReserve);
	const acquiredQuoteAssetAmount = calculateQuoteAssetAmountSwapped(
		acquiredQuote.abs(),
		amm.pegMultiplier,
		swapDirection
	);

	return [acquiredBase, acquiredQuote, acquiredQuoteAssetAmount];
}

/**
 * calculateTargetPriceTrade
 * simple function for finding arbitraging trades
 * @param market
 * @param targetPrice
 * @param pct optional default is 100% gap filling, can set smaller.
 * @param outputAssetType which asset to trade.
 * @param useSpread whether or not to consider the spread when calculating the trade size
 * @returns trade direction/size in order to push price to a targetPrice,
 *
 * [
 *   direction => direction of trade required, PositionDirection
 *   tradeSize => size of trade required, TODO-PRECISION
 *   entryPrice => the entry price for the trade, PRICE_PRECISION
 *   targetPrice => the target price PRICE_PRECISION
 * ]
 */
export function calculateTargetPriceTrade(
	market: PerpMarketAccount,
	targetPrice: BN,
	pct: BN = MAXPCT,
	outputAssetType: AssetType = 'quote',
	oraclePriceData?: OraclePriceData,
	useSpread = true
): [PositionDirection, BN, BN, BN] {
	assert(market.amm.baseAssetReserve.gt(ZERO));
	assert(targetPrice.gt(ZERO));
	assert(pct.lte(MAXPCT) && pct.gt(ZERO));

	const reservePriceBefore = calculateReservePrice(market, oraclePriceData);
	const bidPriceBefore = calculateBidPrice(market, oraclePriceData);
	const askPriceBefore = calculateAskPrice(market, oraclePriceData);

	let direction;
	if (targetPrice.gt(reservePriceBefore)) {
		const priceGap = targetPrice.sub(reservePriceBefore);
		const priceGapScaled = priceGap.mul(pct).div(MAXPCT);
		targetPrice = reservePriceBefore.add(priceGapScaled);
		direction = PositionDirection.LONG;
	} else {
		const priceGap = reservePriceBefore.sub(targetPrice);
		const priceGapScaled = priceGap.mul(pct).div(MAXPCT);
		targetPrice = reservePriceBefore.sub(priceGapScaled);
		direction = PositionDirection.SHORT;
	}

	let tradeSize;
	let baseSize;

	let baseAssetReserveBefore: BN;
	let quoteAssetReserveBefore: BN;

	let peg = market.amm.pegMultiplier;

	if (useSpread && market.amm.baseSpread > 0) {
		const { baseAssetReserve, quoteAssetReserve, newPeg } =
			calculateUpdatedAMMSpreadReserves(market.amm, direction, oraclePriceData);
		baseAssetReserveBefore = baseAssetReserve;
		quoteAssetReserveBefore = quoteAssetReserve;
		peg = newPeg;
	} else {
		baseAssetReserveBefore = market.amm.baseAssetReserve;
		quoteAssetReserveBefore = market.amm.quoteAssetReserve;
	}

	const invariant = market.amm.sqrtK.mul(market.amm.sqrtK);
	const k = invariant.mul(PRICE_PRECISION);

	let baseAssetReserveAfter;
	let quoteAssetReserveAfter;
	const biasModifier = new BN(1);
	let markPriceAfter;

	if (
		useSpread &&
		targetPrice.lt(askPriceBefore) &&
		targetPrice.gt(bidPriceBefore)
	) {
		// no trade, market is at target
		if (reservePriceBefore.gt(targetPrice)) {
			direction = PositionDirection.SHORT;
		} else {
			direction = PositionDirection.LONG;
		}
		tradeSize = ZERO;
		return [direction, tradeSize, targetPrice, targetPrice];
	} else if (reservePriceBefore.gt(targetPrice)) {
		// overestimate y2
		baseAssetReserveAfter = squareRootBN(
			k.div(targetPrice).mul(peg).div(PEG_PRECISION).sub(biasModifier)
		).sub(new BN(1));
		quoteAssetReserveAfter = k.div(PRICE_PRECISION).div(baseAssetReserveAfter);

		markPriceAfter = calculatePrice(
			baseAssetReserveAfter,
			quoteAssetReserveAfter,
			peg
		);
		direction = PositionDirection.SHORT;
		tradeSize = quoteAssetReserveBefore
			.sub(quoteAssetReserveAfter)
			.mul(peg)
			.div(PEG_PRECISION)
			.div(AMM_TO_QUOTE_PRECISION_RATIO);
		baseSize = baseAssetReserveAfter.sub(baseAssetReserveBefore);
	} else if (reservePriceBefore.lt(targetPrice)) {
		// underestimate y2
		baseAssetReserveAfter = squareRootBN(
			k.div(targetPrice).mul(peg).div(PEG_PRECISION).add(biasModifier)
		).add(new BN(1));
		quoteAssetReserveAfter = k.div(PRICE_PRECISION).div(baseAssetReserveAfter);

		markPriceAfter = calculatePrice(
			baseAssetReserveAfter,
			quoteAssetReserveAfter,
			peg
		);

		direction = PositionDirection.LONG;
		tradeSize = quoteAssetReserveAfter
			.sub(quoteAssetReserveBefore)
			.mul(peg)
			.div(PEG_PRECISION)
			.div(AMM_TO_QUOTE_PRECISION_RATIO);
		baseSize = baseAssetReserveBefore.sub(baseAssetReserveAfter);
	} else {
		// no trade, market is at target
		direction = PositionDirection.LONG;
		tradeSize = ZERO;
		return [direction, tradeSize, targetPrice, targetPrice];
	}

	let tp1 = targetPrice;
	let tp2 = markPriceAfter;
	let originalDiff = targetPrice.sub(reservePriceBefore);

	if (direction == PositionDirection.SHORT) {
		tp1 = markPriceAfter;
		tp2 = targetPrice;
		originalDiff = reservePriceBefore.sub(targetPrice);
	}

	const entryPrice = tradeSize
		.mul(AMM_TO_QUOTE_PRECISION_RATIO)
		.mul(PRICE_PRECISION)
		.div(baseSize.abs());

	assert(tp1.sub(tp2).lte(originalDiff), 'Target Price Calculation incorrect');
	assert(
		tp2.lte(tp1) || tp2.sub(tp1).abs() < 100000,
		'Target Price Calculation incorrect' +
			tp2.toString() +
			'>=' +
			tp1.toString() +
			'err: ' +
			tp2.sub(tp1).abs().toString()
	);
	if (outputAssetType == 'quote') {
		return [direction, tradeSize, entryPrice, targetPrice];
	} else {
		return [direction, baseSize, entryPrice, targetPrice];
	}
}

/**
 * Calculates the estimated entry price and price impact of order, in base or quote
 * Price impact is based on the difference between the entry price and the best bid/ask price (whether it's dlob or vamm)
 *
 * @param assetType
 * @param amount
 * @param direction
 * @param market
 * @param oraclePriceData
 * @param dlob
 * @param slot
 * @param usersToSkip
 */
export function calculateEstimatedPerpEntryPrice(
	assetType: AssetType,
	amount: BN,
	direction: PositionDirection,
	market: PerpMarketAccount,
	oraclePriceData: OraclePriceData,
	dlob: DLOB,
	slot: number,
	usersToSkip = new Map<PublicKey, boolean>()
): [BN, BN, BN, BN] {
	if (amount.eq(ZERO)) {
		return [ZERO, ZERO, ZERO, ZERO];
	}

	const takerIsLong = isVariant(direction, 'long');
	const limitOrders = dlob[takerIsLong ? 'getLimitAsks' : 'getLimitBids'](
		market.marketIndex,
		slot,
		MarketType.PERP,
		oraclePriceData
	);

	const swapDirection = getSwapDirection(assetType, direction);

	const { baseAssetReserve, quoteAssetReserve, sqrtK, newPeg } =
		calculateUpdatedAMMSpreadReserves(market.amm, direction, oraclePriceData);
	const amm = {
		baseAssetReserve,
		quoteAssetReserve,
		sqrtK: sqrtK,
		pegMultiplier: newPeg,
	};

	const invariant = amm.sqrtK.mul(amm.sqrtK);

	let bestPrice = calculatePrice(
		amm.baseAssetReserve,
		amm.quoteAssetReserve,
		amm.pegMultiplier
	);

	let cumulativeBaseFilled = ZERO;
	let cumulativeQuoteFilled = ZERO;

	let limitOrder = limitOrders.next().value;
	if (limitOrder) {
		const limitOrderPrice = limitOrder.getPrice(oraclePriceData, slot);
		bestPrice = takerIsLong
			? BN.min(limitOrderPrice, bestPrice)
			: BN.max(limitOrderPrice, bestPrice);
	}

	let worstPrice = bestPrice;

	if (assetType === 'base') {
		while (!cumulativeBaseFilled.eq(amount)) {
			const limitOrderPrice = limitOrder?.getPrice(oraclePriceData, slot);

			let maxAmmFill: BN;
			if (limitOrderPrice) {
				const newBaseReserves = squareRootBN(
					invariant
						.mul(PRICE_PRECISION)
						.mul(amm.pegMultiplier)
						.div(limitOrderPrice)
						.div(PEG_PRECISION)
				);

				// will be zero if the limit order price is better than the amm price
				maxAmmFill = takerIsLong
					? amm.baseAssetReserve.sub(newBaseReserves)
					: newBaseReserves.sub(amm.baseAssetReserve);
			} else {
				maxAmmFill = amount.sub(cumulativeBaseFilled);
			}

			if (maxAmmFill.gt(ZERO)) {
				const baseFilled = BN.min(amount.sub(cumulativeBaseFilled), maxAmmFill);
				const [afterSwapQuoteReserves, afterSwapBaseReserves] =
					calculateAmmReservesAfterSwap(amm, 'base', baseFilled, swapDirection);

				const quoteFilled = calculateQuoteAssetAmountSwapped(
					amm.quoteAssetReserve.sub(afterSwapQuoteReserves).abs(),
					amm.pegMultiplier,
					swapDirection
				);

				cumulativeBaseFilled = cumulativeBaseFilled.add(baseFilled);
				cumulativeQuoteFilled = cumulativeQuoteFilled.add(quoteFilled);

				amm.baseAssetReserve = afterSwapBaseReserves;
				amm.quoteAssetReserve = afterSwapQuoteReserves;

				if (cumulativeBaseFilled.eq(amount)) {
					worstPrice = calculatePrice(
						amm.baseAssetReserve,
						amm.quoteAssetReserve,
						amm.pegMultiplier
					);

					break;
				}
			}

			if (limitOrder && usersToSkip.has(limitOrder.userAccount)) {
				continue;
			}

			const baseFilled = BN.min(
				limitOrder.order.baseAssetAmount.sub(
					limitOrder.order.baseAssetAmountFilled
				),
				amount.sub(cumulativeBaseFilled)
			);
			const quoteFilled = baseFilled.mul(limitOrderPrice).div(BASE_PRECISION);

			cumulativeBaseFilled = cumulativeBaseFilled.add(baseFilled);
			cumulativeQuoteFilled = cumulativeQuoteFilled.add(quoteFilled);

			if (cumulativeBaseFilled.eq(amount)) {
				worstPrice = limitOrderPrice;
				break;
			}

			limitOrder = limitOrders.next().value;
		}
	} else {
		while (!cumulativeQuoteFilled.eq(amount)) {
			const limitOrderPrice = limitOrder?.getPrice(oraclePriceData, slot);

			let maxAmmFill: BN;
			if (limitOrderPrice) {
				const newQuoteReserves = squareRootBN(
					invariant
						.mul(PEG_PRECISION)
						.mul(limitOrderPrice)
						.div(amm.pegMultiplier)
						.div(PRICE_PRECISION)
				);

				// will be zero if the limit order price is better than the amm price
				maxAmmFill = takerIsLong
					? newQuoteReserves.sub(amm.quoteAssetReserve)
					: amm.quoteAssetReserve.sub(newQuoteReserves);
			} else {
				maxAmmFill = amount.sub(cumulativeQuoteFilled);
			}

			if (maxAmmFill.gt(ZERO)) {
				const quoteFilled = BN.min(
					amount.sub(cumulativeQuoteFilled),
					maxAmmFill
				);
				const [afterSwapQuoteReserves, afterSwapBaseReserves] =
					calculateAmmReservesAfterSwap(
						amm,
						'quote',
						quoteFilled,
						swapDirection
					);

				const baseFilled = afterSwapBaseReserves
					.sub(amm.baseAssetReserve)
					.abs();

				cumulativeBaseFilled = cumulativeBaseFilled.add(baseFilled);
				cumulativeQuoteFilled = cumulativeQuoteFilled.add(quoteFilled);

				amm.baseAssetReserve = afterSwapBaseReserves;
				amm.quoteAssetReserve = afterSwapQuoteReserves;

				if (cumulativeQuoteFilled.eq(amount)) {
					worstPrice = calculatePrice(
						amm.baseAssetReserve,
						amm.quoteAssetReserve,
						amm.pegMultiplier
					);
					break;
				}
			}

			if (limitOrder && usersToSkip.has(limitOrder.userAccount)) {
				continue;
			}

			const quoteFilled = BN.min(
				limitOrder.order.baseAssetAmount
					.sub(limitOrder.order.baseAssetAmountFilled)
					.mul(limitOrderPrice)
					.div(BASE_PRECISION),
				amount.sub(cumulativeQuoteFilled)
			);

			const baseFilled = quoteFilled.mul(BASE_PRECISION).div(limitOrderPrice);

			cumulativeBaseFilled = cumulativeBaseFilled.add(baseFilled);
			cumulativeQuoteFilled = cumulativeQuoteFilled.add(quoteFilled);

			if (cumulativeQuoteFilled.eq(amount)) {
				worstPrice = limitOrderPrice;
				break;
			}

			limitOrder = limitOrders.next().value;
		}
	}

	const entryPrice = cumulativeQuoteFilled
		.mul(BASE_PRECISION)
		.div(cumulativeBaseFilled);

	const priceImpact = entryPrice
		.sub(bestPrice)
		.mul(PRICE_PRECISION)
		.div(bestPrice)
		.abs();

	return [entryPrice, priceImpact, bestPrice, worstPrice];
}
