if [ "$1" != "--skip-build" ]
  then
    anchor build &&
    cp target/idl/clearing_house.json sdk/src/idl/
fi

test_files=(bankDepositWithdraw.ts clearingHouse.ts ordersWithSpread.ts order.ts orderReferrer.ts marketOrder.ts triggerOrders.ts stopLimits.ts userOrderId.ts makerOrder.ts roundInFavorBaseAsset.ts marketOrderBaseAssetAmount.ts oracleOffsetOrders.ts pyth.ts userAccount.ts admin.ts updateK.ts adminWithdraw.ts curve.ts fees.ts idempotentCurve.ts maxPositions.ts maxReserves.ts twapDivergenceLiquidation.ts oraclePnlLiquidation.ts whaleLiquidation.ts roundInFavor.ts minimumTradeSize.ts cappedSymFunding.ts)

for test_file in ${test_files[@]}; do
  export ANCHOR_TEST_FILE=${test_file} && anchor test --skip-build || exit 1;
done
