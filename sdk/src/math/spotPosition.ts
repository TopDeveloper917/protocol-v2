import { SpotMarketAccount, SpotPosition } from '../types';
import { ZERO } from '../constants/numericConstants';
import { BN } from '@project-serum/anchor';
import {
	getSignedTokenAmount,
	getTokenAmount,
	getTokenValue,
} from './spotBalance';
import { OraclePriceData } from '../oracles/types';

export function isSpotPositionAvailable(position: SpotPosition): boolean {
	return position.balance.eq(ZERO) && position.openOrders === 0;
}

export function getWorstCaseTokenAmounts(
	spotPosition: SpotPosition,
	spotMarketAccount: SpotMarketAccount,
	oraclePriceData: OraclePriceData
): [BN, BN] {
	const tokenAmount = getSignedTokenAmount(
		getTokenAmount(
			spotPosition.balance,
			spotMarketAccount,
			spotPosition.balanceType
		),
		spotPosition.balanceType
	);

	const tokenAmountAllBidsFill = tokenAmount.add(spotPosition.openBids);
	const tokenAmountAllAsksFill = tokenAmount.add(spotPosition.openAsks);

	if (tokenAmountAllAsksFill.abs().gt(tokenAmountAllBidsFill.abs())) {
		const worstCaseQuoteTokenAmount = getTokenValue(
			spotPosition.openBids.neg(),
			spotMarketAccount.decimals,
			oraclePriceData
		);
		return [tokenAmountAllBidsFill, worstCaseQuoteTokenAmount];
	} else {
		const worstCaseQuoteTokenAmount = getTokenValue(
			spotPosition.openAsks.neg(),
			spotMarketAccount.decimals,
			oraclePriceData
		);
		return [tokenAmountAllAsksFill, worstCaseQuoteTokenAmount];
	}
}
