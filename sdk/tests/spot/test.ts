import { BN, ZERO, calculateSpotMarketBorrowCapacity } from '../../src';
import { mockSpotMarkets } from '../dlob/helpers';

import { assert } from '../../src/assert/assert';
import { SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION } from '@drift-labs/sdk';

describe('Spot Tests', () => {
	it('base borrow capacity', () => {
        const mockSpot = mockSpotMarkets[0];
        mockSpot.maxBorrowRate = 1000000;
        mockSpot.optimalBorrowRate = 100000;
        mockSpot.optimalUtilization = 700000;

        mockSpot.decimals = 9;
        mockSpot.cumulativeDepositInterest = SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION;
        mockSpot.cumulativeBorrowInterest = SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION;

        const tokenAmount = 100000;
        // no borrows
        mockSpot.depositBalance = new BN(tokenAmount * 1e9);
        mockSpot.borrowBalance = ZERO; 

        // todo, should incorp all other spot market constraints?
        const aboveMaxAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(2000000));
        assert(aboveMaxAmount.gt(mockSpot.depositBalance));

        const maxAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(1000000));
        assert(maxAmount.eq(mockSpot.depositBalance));

        const optAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(100000));
        const ans = new BN(mockSpot.depositBalance.toNumber() * 7 / 10);
        // console.log('optAmount:', optAmount.toNumber(), ans.toNumber());
        assert(optAmount.eq(ans));

        const betweenOptMaxAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(810000));
        // console.log('betweenOptMaxAmount:', betweenOptMaxAmount.toNumber());
        assert(betweenOptMaxAmount.lt(mockSpot.depositBalance));
        assert(betweenOptMaxAmount.gt(ans));
        assert(betweenOptMaxAmount.eq(new BN(93666600000000)));

        const belowOptAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(50000));
        // console.log('belowOptAmount:', belowOptAmount.toNumber());
        assert(belowOptAmount.eq(ans.div(new BN(2))));

        const belowOptAmount2 = calculateSpotMarketBorrowCapacity(mockSpot, new BN(24900));
        // console.log('belowOptAmount2:', belowOptAmount2.toNumber());
        assert(belowOptAmount2.lt(ans.div(new BN(4))));
        assert(belowOptAmount2.eq(new BN('17430000000000')));

        const belowOptAmount3 = calculateSpotMarketBorrowCapacity(mockSpot, new BN(1));
        // console.log('belowOptAmount3:', belowOptAmount3.toNumber());
        assert(belowOptAmount3.eq(new BN('700000000'))); //0.7
    });


    it('complex borrow capacity', () => {
        const mockSpot = mockSpotMarkets[0];
        mockSpot.maxBorrowRate = 1000000;
        mockSpot.optimalBorrowRate =  70000;
        mockSpot.optimalUtilization = 700000;

        mockSpot.decimals = 9;
        mockSpot.cumulativeDepositInterest = new BN(1.0154217042 * SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION.toNumber());
        mockSpot.cumulativeBorrowInterest = new BN(1.0417153549 * SPOT_MARKET_CUMULATIVE_INTEREST_PRECISION.toNumber());

        mockSpot.depositBalance = new BN(88522.734106451 * 1e9);
        mockSpot.borrowBalance = new BN(7089.91675884 * 1e9);

        // todo, should incorp all other spot market constraints?
        const aboveMaxAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(2000000));
        assert(aboveMaxAmount.eq(new BN('111498270939007')));

        const maxAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(1000000));
        assert(maxAmount.eq(new BN('82502230374168')));
        // console.log('aboveMaxAmount:', aboveMaxAmount.toNumber(), 'maxAmount:', maxAmount.toNumber());
        const optAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(70000));
        // console.log('optAmount:', optAmount.toNumber());
        assert(optAmount.eq(new BN('55535858716123'))); // ~ 55535

        const betweenOptMaxAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(810000));
        // console.log('betweenOptMaxAmount:', betweenOptMaxAmount.toNumber());
        assert(betweenOptMaxAmount.lt(maxAmount));
        assert(betweenOptMaxAmount.eq(new BN(76992910756523)));
        assert(betweenOptMaxAmount.gt(optAmount));

        const belowOptAmount = calculateSpotMarketBorrowCapacity(mockSpot, new BN(50000));
        // console.log('belowOptAmount:', belowOptAmount.toNumber());
        assert(belowOptAmount.eq(new BN('37558277610760')));

        const belowOptAmount2 = calculateSpotMarketBorrowCapacity(mockSpot, new BN(24900));
        // console.log('belowOptAmount2:', belowOptAmount2.toNumber());
        assert(belowOptAmount2.eq(new BN('14996413323529')));

        const belowOptAmount3 = calculateSpotMarketBorrowCapacity(mockSpot, new BN(4900));
        // console.log('belowOptAmount2:', belowOptAmount3.toNumber());
        assert(belowOptAmount3.eq(new BN('0')));

        const belowOptAmount4 = calculateSpotMarketBorrowCapacity(mockSpot, new BN(1));
        // console.log('belowOptAmount3:', belowOptAmount4.toNumber());
        assert(belowOptAmount4.eq(new BN('0')));
    });
});