import { BN, OracleSource } from '../';

export type MarketConfig = {
	fullName?: string;
	category?: string[];
	symbol: string;
	baseAssetSymbol: string;
	marketIndex: BN;
	devnetPythOracle: string;
	mainnetPythOracle: string;
	launchTs: number;
	oracleSource: OracleSource;
};

export const Markets: MarketConfig[] = [
	{
		fullName: 'Solana',
		category: ['L1', 'Infra'],
		symbol: 'SOL-PERP',
		baseAssetSymbol: 'SOL',
		marketIndex: new BN(0),
		devnetPythOracle: 'J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix',
		mainnetPythOracle: 'H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG',
		launchTs: 1635209696886,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Bitcoin',
		category: ['L1', 'Payment'],
		symbol: 'BTC-PERP',
		baseAssetSymbol: 'BTC',
		marketIndex: new BN(1),
		devnetPythOracle: 'HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J',
		mainnetPythOracle: 'GVXRSBjFk6e6J3NbVPXohDJetcTjaeeuykUpbQF8UoMU',
		launchTs: 1637691088868,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Ethereum',
		category: ['L1', 'Infra'],
		symbol: 'ETH-PERP',
		baseAssetSymbol: 'ETH',
		marketIndex: new BN(2),
		devnetPythOracle: 'EdVCmQ9FSPcVe5YySXDPCRmc8aDQLKJ9xvYBMZPie1Vw',
		mainnetPythOracle: 'JBu1AL4obBcCMqKBBxhpWCNUt136ijcuMZLFvTP7iWdB',
		launchTs: 1637691133472,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Terra',
		category: ['L1', 'Infra'],
		symbol: 'LUNA-PERP',
		baseAssetSymbol: 'LUNA',
		marketIndex: new BN(3),
		devnetPythOracle: '8PugCXTAHLM9kfLSQWe2njE5pzAgUdpPk3Nx5zSm7BD3',
		mainnetPythOracle: '5bmWuR1dgP4avtGYMNKLuxumZTVKGgoN2BCMXWDNL9nY',
		launchTs: 1638821738525,
		oracleSource: OracleSource.PYTH,
	},
	{
		symbol: 'AVAX-PERP',
		category: ['L1', 'Infra'],
		baseAssetSymbol: 'AVAX',
		marketIndex: new BN(4),
		devnetPythOracle: 'FVb5h1VmHPfVb1RfqZckchq18GxRv4iKt8T4eVTQAqdz',
		mainnetPythOracle: 'Ax9ujW5B9oqcv59N8m6f1BpTBq2rGeGaBcpKjC5UYsXU',
		launchTs: 1639092501080,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: "Binance Coin",
		symbol: 'BNB-PERP',
		category: ['L1', 'Exchange'],
		baseAssetSymbol: 'BNB',
		marketIndex: new BN(5),
		devnetPythOracle: 'GwzBgrXb4PG59zjce24SF2b9JXbLEjJJTBkmytuEZj1b',
		mainnetPythOracle: '4CkQJBxhU8EZ2UjhigbtdaPbpTe6mqf811fipYBFbSYN',
		launchTs: 1639523193012,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: "Polygon",
		category: ['L1', 'Infra'],
		symbol: 'MATIC-PERP',
		baseAssetSymbol: 'MATIC',
		marketIndex: new BN(6),
		devnetPythOracle: 'FBirwuDFuRAu4iSGc7RGxN5koHB7EJM1wbCmyPuQoGur',
		mainnetPythOracle: '7KVswB9vkCgeM3SHP7aGDijvdRAHK8P5wi9JXViCrtYh',
		launchTs: 1641488603564,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Cosmos',
		category: ['L1', 'Infra'],
		symbol: 'ATOM-PERP',
		baseAssetSymbol: 'ATOM',
		marketIndex: new BN(7),
		devnetPythOracle: '7YAze8qFUMkBnyLVdKT4TFUUFui99EwS5gfRArMcrvFk',
		mainnetPythOracle: 'CrCpTerNqtZvqLcKqz1k13oVeXV9WkMD2zA9hBKXrsbN',
		launchTs: 1641920238195,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: "Polkadot",
		symbol: 'DOT-PERP',
		baseAssetSymbol: 'DOT',
		marketIndex: new BN(8),
		devnetPythOracle: '4dqq5VBpN4EwYb7wyywjjfknvMKu7m78j9mKZRXTj462',
		mainnetPythOracle: 'EcV1X1gY2yb4KXxjVQtTHTbioum2gvmPnFk4zYAt7zne',
		launchTs: 1642629253786,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Cardano',
		category: ['L1', 'Infra'],
		symbol: 'ADA-PERP',
		baseAssetSymbol: 'ADA',
		marketIndex: new BN(9),
		devnetPythOracle: '8oGTURNmSQkrBS1AQ5NjB2p8qY34UVmMA9ojrw8vnHus',
		mainnetPythOracle: '3pyn4svBbxJ9Wnn3RVeafyLWfzie6yC5eTig2S62v9SC',
		launchTs: 1643084413000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: "Algorand",
		category: ['L1', 'Infra'],
		symbol: 'ALGO-PERP',
		baseAssetSymbol: 'ALGO',
		marketIndex: new BN(10),
		devnetPythOracle: 'c1A946dY5NHuVda77C8XXtXytyR3wK1SCP3eA9VRfC3',
		mainnetPythOracle: 'HqFyq1wh1xKvL7KDqqT7NJeSPdAqsDqnmBisUC2XdXAX',
		launchTs: 1643686767000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'FTX Coin',
		category: ['L1', 'Exchange'],
		symbol: 'FTT-PERP',
		baseAssetSymbol: 'FTT',
		marketIndex: new BN(11),
		devnetPythOracle: '6vivTRs5ZPeeXbjo7dfburfaYDWoXjBtdtuYgQRuGfu',
		mainnetPythOracle: '8JPJJkmDScpcNmBRKGZuPuG2GYAveQgP3t5gFuMymwvF',
		launchTs: 1644382122000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Litecoin',
		category: ['L1', 'Payments'],
		symbol: 'LTC-PERP',
		baseAssetSymbol: 'LTC',
		marketIndex: new BN(12),
		devnetPythOracle: 'BLArYBCUYhdWiY8PCUTpvFE21iaJq85dvxLk9bYMobcU',
		mainnetPythOracle: '8RMnV1eD55iqUFJLMguPkYBkq8DCtx81XcmAja93LvRR',
		launchTs: 1645027429000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Ripple',
		category: ['L1', 'Payments'],		
		symbol: 'XRP-PERP',
		baseAssetSymbol: 'XRP',
		marketIndex: new BN(13),
		devnetPythOracle: 'WMW5xc3HypXwTnPesyUT49uLsyHwNURsWAEk39onKuk',
		mainnetPythOracle: 'WMW5xc3HypXwTnPesyUT49uLsyHwNURsWAEk39onKuk',
		launchTs: 1647543166000,
		oracleSource: OracleSource.SWITCHBOARD,
	},
	// {
	// 	symbol: 'mSOL-PERP',
	// 	baseAssetSymbol: 'mSOL',
	// 	marketIndex: new BN(11), //todo
	// 	devnetPythOracle: '9a6RNx3tCu1TSs6TBSfV2XRXEPEZXQ6WB7jRojZRvyeZ',
	// 	mainnetPythOracle: 'E4v1BBgoso9s64TQvmyownAVJbhbEPGyzA3qn4n46qj9',
	// 	launchTs: 1643346125000,
	// },
];
