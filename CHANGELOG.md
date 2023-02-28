# Changelog


All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Features

- program: max number of subaccounts to 3000
- program: amm spread logic more consistent across market by using liquidity ratio rather than base asset amount for inventory spread scaling([#374](https://github.com/drift-labs/protocol-v2/pull/374))
- program: add pyth1M/pyth1K as OracleSource ([#375](https://github.com/drift-labs/protocol-v2/pull/375))

### Fixes

### Breaking

## [2.18.0] - 2023-02-24

### Features

- program: account for contract tier in liquidate_perp_pnl_for_deposit ([#368](https://github.com/drift-labs/protocol-v2/pull/368))
- program: simplifications for order fills ([#370](https://github.com/drift-labs/protocol-v2/pull/370))
- program: block atomic fills ([#369](https://github.com/drift-labs/protocol-v2/pull/369))
- program: allow limit orders to go through auction ([#355](https://github.com/drift-labs/protocol-v2/pull/355))
- program: improve conditions for withdraw/borrow guard ([#354](https://github.com/drift-labs/protocol-v2/pull/354))

### Fixes

- ts-sdk: fix resolvePerpBankrupcty to work with all perp market indexes
- ts-sdk: getTokenAmount uses divCeil ([#371](https://github.com/drift-labs/protocol-v2/pull/371))
- program: allow limit orders to have explicit zero auction duration passed in params ([#373](https://github.com/drift-labs/protocol-v2/pull/373))

### Breaking

## [2.17.0] - 2023-02-17

### Features

- program: order params utilize post only enum ([#361](https://github.com/drift-labs/protocol-v2/pull/361))

### Fixes
- program: twap tweaks, update only on new cluster time ([#362](https://github.com/drift-labs/protocol-v2/pull/362))

### Breaking

## [2.16.0] - 2023-02-14

### Features

- sdk: add support for market lookup table ([#359](https://github.com/drift-labs/protocol-v2/pull/359))
- program: tweak calculate_size_premium_liability_weight to have smaller effect on initial margin ([#350](https://github.com/drift-labs/protocol-v2/pull/350))
- ts-sdk: updates for accounting for spot leverage ([#295](https://github.com/drift-labs/protocol-v2/pull/295))
- ts-sdk: added new methods for modifying orders to include spot and more params ([#353](https://github.com/drift-labs/protocol-v2/pull/353))
- ts-sdk: flagged old modifyPerpOrder and modifyPerpOrderByUserOrderId as deprecated

### Fixes

- ts-sdk: DLOB matching logic accounts for zero-price spot market orders not matching resting limit orders
- ts-sdk: new squareRootBN implementation using bit shifting (2x speed improvement)
- program: fix overflow in calculate_long_short_vol_spread ([#352](https://github.com/drift-labs/protocol-v2/pull/352))
- program: dont let users disable margin trading if they have margin orders open 
- program: tweaks to fix max leverage order param flag with imf factor ([#351](https://github.com/drift-labs/protocol-v2/pull/351))
- program: improve bid/ask twap calculation for funding rate stability ([#345](https://github.com/drift-labs/protocol-v2/pull/345))
- ts-sdk: fix borrow limit calc ([#356](https://github.com/drift-labs/protocol-v2/pull/356))

### Breaking

## [2.15.0] - 2023-02-07

### Features

- ts-sdk: add aptos

### Fixes 

### Breaking

## [2.14.0] - 2023-02-06

### Features

- program: flag to set max leverage for orders ([#346](https://github.com/drift-labs/protocol-v2/pull/346))
- program: do imf size discount for maintainance spot asset weight ([#343](https://github.com/drift-labs/protocol-v2/pull/343))
- ts-sdk: new liquidation price to account for delta neutral strategies ([#340](https://github.com/drift-labs/protocol-v2/pull/340))
- ts-sdk: add txParams to all instructions, bump @solana/web3.js ([#344](https://github.com/drift-labs/protocol-v2/pull/344))

### Fixes

- program: extend time before limit order is considered resting ([#349](https://github.com/drift-labs/protocol-v2/pull/349))
- ts-sdk: improve funding rate prediction
- program: block jit maker orders from cross vamm
- program: cancel_order_by_user_order_id fails if order is not found

### Breaking

## [2.13.0] - 2023-01-31

### Features

- program: perp bankruptcies pay from fee pool before being socialized ([#332](https://github.com/drift-labs/protocol-v2/pull/332))
- ts-sdk: add calculateAvailablePerpLiquidity
- program: enforce min order size when trading against amm ([#334](https://github.com/drift-labs/protocol-v2/pull/334))

### Fixes

- ts-sdk: fix the getBuyingPower calculation
- ts-sdk: improved perp estimated liq price formula ([#338](https://github.com/drift-labs/protocol-v2/pull/338))
- ts-sdk: update methods to account for new leverage formula ([#339](https://github.com/drift-labs/protocol-v2/pull/339))

### Breaking

## [2.12.0] - 2023-01-22

### Features

- program: allow for 2000 users
- program: add resting limit order logic ([#328](https://github.com/drift-labs/protocol-v2/pull/328))
- ts-sdk: add calculateEstimatedSpotEntryPrice
- ts-sdk: add ability to add priority fees ([#331](https://github.com/drift-labs/protocol-v2/pull/331))
- ts-sdk: new calculateEstimatedPerpEntryPrice that accounts for dlob & vamm ([#326](https://github.com/drift-labs/protocol-v2/pull/326))

### Fixes

- program: better rounding for openbook limit price
- program: fix paying fee_pool_delta when filling with open book
- program: bitflags for exchange status ([#330](https://github.com/drift-labs/protocol-v2/pull/330))
- program: update fee calculation for filling against openbook
- program: relax conditions for valid oracle price in fulfill_perp_order
- program: handle fallback price when amm has no liquidity ([#324](https://github.com/drift-labs/protocol-v2/pull/324))
- sdk: add getRestingLimitBids/Asks to DLOB ([#325](https://github.com/drift-labs/protocol-v2/pull/325))
- program: tweak oracle price used for determine_perp_fulfillment_methods

### Breaking

## [2.11.0] - 2023-01-11

### Features

- program: remove canceling market orders with limit price after first fill
- program: try to match against multiple of makers orders ([#315](https://github.com/drift-labs/protocol-v2/pull/316))
- program: limit number of users to 1500
- program: more rigorous risk decreasing check in place_perp_order/place_stop_order

### Fixes

- program: avoid overflow when calculating overflow ([#322](https://github.com/drift-labs/protocol-v2/pull/322))
- ts-sdk: fix user.getUnrealizedPnl to account for lp position
- program: cancel market order for not satisfying limit price only if there was some base asset amount filled

### Breaking

## [2.10.0] - 2023-01-03

### Features

- program: place order returns early if max ts breached ([#317](https://github.com/drift-labs/protocol-v2/pull/317))
- ts-sdk: batch getMultipleAccount calls in bulkAccountLoader ([#315](https://github.com/drift-labs/protocol-v2/pull/315))
- program: add clippy deny for panic, expect and unwrap
- program: add market index offset trait ([#287](https://github.com/drift-labs/protocol-v2/pull/287))
- program: add size trait to accounts and events ([#286](https://github.com/drift-labs/protocol-v2/pull/286))

### Fixes

- program: add access control for spot market updates similar to perp market ([#284](https://github.com/drift-labs/protocol-v2/pull/284))
- ts-sdk: allow websocket subscriber to skip getAccount call to rpc ([#313](https://github.com/drift-labs/protocol-v2/pull/313))
- ts-sdk: always add market account for cancelOrders if market index included
- anchor tests: make deterministic to run in ci ([#289](https://github.com/drift-labs/protocol-v2/pull/289))
- ts-sdk: fix deprecated calls to `@solana/web3.js` ([#299](https://github.com/drift-labs/protocol-v2/pull/307))
- ts-sdk: fix calculateAssetWeight for Maintenance Margin ([#308](https://github.com/drift-labs/protocol-v2/pull/308))
- ts-sdk: fix UserMap for websocket usage ([#308](https://github.com/drift-labs/protocol-v2/pull/310))

### Breaking

## [2.9.0] - 2022-12-23

### Features

- program: use vamm price to guard against bad fills for limit orders ([#304](https://github.com/drift-labs/protocol-v2/pull/304))

### Fixes

- ts-sdk: expect signTransaction from wallet adapters to return a copy ([#299](https://github.com/drift-labs/protocol-v2/pull/299))

### Breaking

## [2.8.0] - 2022-12-22

### Features

- program: add force_cancel_orders to cancel risk-increasing orders for users with excessive leverage ([#298](https://github.com/drift-labs/protocol-v2/pull/298))

### Fixes

- program: fix calculate_availability_borrow_liquidity ([#301](https://github.com/drift-labs/protocol-v2/pull/301))
- program: fix casting in fulfill_spot_order_with_match to handle implied max_base_asset_amounts
- sdk: fix BulkAccountLoader starvation ([#300](https://github.com/drift-labs/protocol-v2/pull/300))

### Breaking

## [2.7.0] - 2022-12-19

### Features

### Fixes

program: more leniency in allowing risk decreasing trades for perps ([#297](https://github.com/drift-labs/protocol-v2/pull/297))
program: fix is_user_being_liquidated in deposit

### Breaking

## [2.6.0] - 2022-12-16

### Features

program: allow keeper to switch user status to active by calling liquidate perp ([#296](https://github.com/drift-labs/protocol-v2/pull/296))

### Fixes

- program: more precise update k in prepeg ([#294](https://github.com/drift-labs/protocol-v2/pull/294))
- program: allow duplicative reduce only orders ([#293](https://github.com/drift-labs/protocol-v2/pull/293))
- program: fix should_cancel_reduce_only_order
- ts-sdk: add Oracle OrderType to dlob idl

### Breaking

## [2.5.0] - 2022-12-13

### Features

### Fixes

- program: disable lower bound check for update amm once it's already been breached ([#292](https://github.com/drift-labs/protocol-v2/pull/292))
- ts-sdk: fix DLOB.updateOrder ([#290](https://github.com/drift-labs/protocol-v2/pull/290))
- ts-sdk: make calculateClaimablePnl mirror on-chain logic ([#291](https://github.com/drift-labs/protocol-v2/pull/291))
- ts-sdk: add margin trading toggle field to user accounts, update toggle margin trading function to add ability to toggle for any subaccount rather than just the active ([#285](https://github.com/drift-labs/protocol-v2/pull/285))

### Breaking

## [2.4.0] - 2022-12-09

### Features

- program: check if place_perp_order can lead to breach in max oi ([#283](https://github.com/drift-labs/protocol-v2/pull/283))
- program: find fallback maker order if passed order id doesnt exist ([#281](https://github.com/drift-labs/protocol-v2/pull/281))

### Fixes

- program: fix amm-jit so makers can fill the full size of their order after amm-jit occurs ([#280](https://github.com/drift-labs/protocol-v2/pull/280))

### Breaking

## [2.3.0] - 2022-12-07

### Features

### Fixes

- program: update the amm min/max_base_asset_reserve upon k decreases within update_amm ([#282](https://github.com/drift-labs/protocol-v2/pull/282))
- program: fix amm-jit erroring out when bids/asks are zero ([#279](https://github.com/drift-labs/protocol-v2/pull/279))
- ts-sdk: fix overflow in inventorySpreadScale

### Breaking

## [2.2.0] - 2022-12-06

### Features

- ts-sdk: add btc/eth perp market configs for mainnet ([#277](https://github.com/drift-labs/protocol-v2/pull/277))
- program: reduce if stake requirement for better fee tier ([#275](https://github.com/drift-labs/protocol-v2/pull/275))
- program: new oracle order where auction price is oracle price offset ([#269](https://github.com/drift-labs/protocol-v2/pull/269)).
- program: block negative pnl settles which would lead to more borrows when quote spot utilization is high ([#273](https://github.com/drift-labs/protocol-v2/pull/273)).

### Fixes

- ts-sdk: fix bugs in calculateSpreadBN
- ts-sdk: fix additional bug in calculateSpreadBN (negative nums)

### Breaking
