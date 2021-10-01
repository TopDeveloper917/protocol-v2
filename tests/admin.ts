import * as anchor from '@project-serum/anchor';
import {Program} from '@project-serum/anchor';
import BN from 'bn.js';
import { assert } from 'chai';

import {
    ClearingHouse,
} from '../sdk/src';

import {
    mockUSDCMint,
} from '../utils/mockAccounts';
import {PublicKey} from "@solana/web3.js";

describe('clearing_house', () => {
    const provider = anchor.Provider.local();
    const connection = provider.connection;
    anchor.setProvider(provider);
    const chProgram = anchor.workspace.ClearingHouse as Program;

    let clearingHouse: ClearingHouse;

    let usdcMint;

    before(async () => {
        usdcMint = await mockUSDCMint(provider);

        clearingHouse = new ClearingHouse(
            connection,
            provider.wallet,
            chProgram.programId
        );

        await clearingHouse.initialize(
            usdcMint.publicKey,
            false
        );
        await clearingHouse.subscribe();
    });

    it('Update Margin Ratio', async () => {
        const marginRatioInitial = new BN (1);
        const marginRatioPartial = new BN (1);
        const marginRatioMaintenance = new BN (1);

        await clearingHouse.updateMarginRatio(marginRatioInitial, marginRatioPartial, marginRatioMaintenance);

        const state = clearingHouse.getState();

        assert(state.marginRatioInitial.eq(marginRatioInitial));
        assert(state.marginRatioPartial.eq(marginRatioPartial));
        assert(state.marginRatioMaintenance.eq(marginRatioMaintenance));
    });

    it('Update Partial Liquidation Close Percentages', async () => {
        const numerator = new BN (1);
        const denominator = new BN (10);

        await clearingHouse.updatePartialLiquidationClosePercentage(numerator, denominator);

        const state = clearingHouse.getState();

        assert(state.partialLiquidationClosePercentageNumerator.eq(numerator));
        assert(state.partialLiquidationClosePercentageDenominator.eq(denominator));
    });

    it('Update Partial Liquidation Penalty Percentages', async () => {
        const numerator = new BN (1);
        const denominator = new BN (10);

        await clearingHouse.updatePartialLiquidationPenaltyPercentage(numerator, denominator);

        const state = clearingHouse.getState();

        assert(state.partialLiquidationPenaltyPercentageNumerator.eq(numerator));
        assert(state.partialLiquidationPenaltyPercentageDenominator.eq(denominator));
    });

    it('Update Full Liquidation Penalty Percentages', async () => {
        const numerator = new BN (1);
        const denominator = new BN (10);

        await clearingHouse.updateFullLiquidationPenaltyPercentage(numerator, denominator);

        const state = clearingHouse.getState();

        assert(state.fullLiquidationPenaltyPercentageNumerator.eq(numerator));
        assert(state.fullLiquidationPenaltyPercentageDenominator.eq(denominator));
    });

    it('Update Partial Liquidation Share Denominator', async () => {
        const denominator = new BN (10);

        await clearingHouse.updatePartialLiquidationShareDenominator(denominator);

        const state = clearingHouse.getState();

        assert(state.partialLiquidationLiquidatorShareDenominator.eq(denominator));
    });

    it('Update Full Liquidation Share Denominator', async () => {
        const denominator = new BN (10);

        await clearingHouse.updateFullLiquidationShareDenominator(denominator);

        const state = clearingHouse.getState();

        assert(state.fullLiquidationLiquidatorShareDenominator.eq(denominator));
    });

    it('Update fee', async () => {
        const numerator = new BN (10);
        const denominator = new BN (10);

        await clearingHouse.updateFee(numerator, denominator);

        const state = clearingHouse.getState();

        assert(state.feeNumerator.eq(numerator));
        assert(state.feeDenominator.eq(denominator));
    });

    it('Update admin', async () => {
        const admin = PublicKey.default;

        await clearingHouse.updateAdmin(admin);

        const state = clearingHouse.getState();

        assert(state.admin.equals(admin));
    });

    after(async () => {
        await clearingHouse.unsubscribe();
    });


});