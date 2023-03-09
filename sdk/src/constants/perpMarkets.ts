import { OracleSource } from '../';
import { DriftEnv } from '../';
import { PublicKey } from '@solana/web3.js';

export type PerpMarketConfig = {
	fullName?: string;
	category?: string[];
	symbol: string;
	baseAssetSymbol: string;
	marketIndex: number;
	launchTs: number;
	oracle: PublicKey;
	oracleSource: OracleSource;
};

export const DevnetPerpMarkets: PerpMarketConfig[] = [
	{
		fullName: 'Solana',
		category: ['L1', 'Infra'],
		symbol: 'SOL-PERP',
		baseAssetSymbol: 'SOL',
		marketIndex: 0,
		oracle: new PublicKey('J83w4HKfqxwcq3BEMMkPFSppX3gqekLyLJBexebFVkix'),
		launchTs: 1655751353000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Bitcoin',
		category: ['L1', 'Payment'],
		symbol: 'BTC-PERP',
		baseAssetSymbol: 'BTC',
		marketIndex: 1,
		oracle: new PublicKey('HovQMDrbAgAYPCmHVSrezcSmkMtXSSUsLDFANExrZh2J'),
		launchTs: 1655751353000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Ethereum',
		category: ['L1', 'Infra'],
		symbol: 'ETH-PERP',
		baseAssetSymbol: 'ETH',
		marketIndex: 2,
		oracle: new PublicKey('EdVCmQ9FSPcVe5YySXDPCRmc8aDQLKJ9xvYBMZPie1Vw'),
		launchTs: 1637691133472,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Aptos',
		category: ['L1', 'Infra'],
		symbol: 'APT-PERP',
		baseAssetSymbol: 'APT',
		marketIndex: 3,
		oracle: new PublicKey('5d2QJ6u2NveZufmJ4noHja5EHs3Bv1DUMPLG5xfasSVs'),
		launchTs: 1675610186000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Bonk',
		category: ['Meme'],
		symbol: '1MBONK-PERP',
		baseAssetSymbol: '1MBONK',
		marketIndex: 4,
		oracle: new PublicKey('6bquU99ktV1VRiHDr8gMhDFt3kMfhCQo5nfNrg2Urvsn'),
		launchTs: 1677068931000,
		oracleSource: OracleSource.PYTH_1M,
	},
	{
		fullName: 'Polygon',
		category: ['L2', 'Infra'],
		symbol: 'MATIC-PERP',
		baseAssetSymbol: 'MATIC',
		marketIndex: 5,
		oracle: new PublicKey('FBirwuDFuRAu4iSGc7RGxN5koHB7EJM1wbCmyPuQoGur'),
		launchTs: 1677690149000, //todo
		oracleSource: OracleSource.PYTH,
	},
];

export const MainnetPerpMarkets: PerpMarketConfig[] = [
	{
		fullName: 'Solana',
		category: ['L1', 'Infra'],
		symbol: 'SOL-PERP',
		baseAssetSymbol: 'SOL',
		marketIndex: 0,
		oracle: new PublicKey('H6ARHf6YXhGYeQfUzQNGk6rDNnLBQKrenN712K4AQJEG'),
		launchTs: 1667560505000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Bitcoin',
		category: ['L1', 'Payment'],
		symbol: 'BTC-PERP',
		baseAssetSymbol: 'BTC',
		marketIndex: 1,
		oracle: new PublicKey('GVXRSBjFk6e6J3NbVPXohDJetcTjaeeuykUpbQF8UoMU'),
		launchTs: 1670347281000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Ethereum',
		category: ['L1', 'Infra'],
		symbol: 'ETH-PERP',
		baseAssetSymbol: 'ETH',
		marketIndex: 2,
		oracle: new PublicKey('JBu1AL4obBcCMqKBBxhpWCNUt136ijcuMZLFvTP7iWdB'),
		launchTs: 1670347281000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Aptos',
		category: ['L1', 'Infra'],
		symbol: 'APT-PERP',
		baseAssetSymbol: 'APT',
		marketIndex: 3,
		oracle: new PublicKey('FNNvb1AFDnDVPkocEri8mWbJ1952HQZtFLuwPiUjSJQ'),
		launchTs: 1675802661000,
		oracleSource: OracleSource.PYTH,
	},
	{
		fullName: 'Bonk',
		category: ['Meme'],
		symbol: '1MBONK-PERP',
		baseAssetSymbol: '1MBONK',
		marketIndex: 4,
		oracle: new PublicKey('8ihFLu5FimgTQ1Unh4dVyEHUGodJ5gJQCrQf4KUVB9bN'),
		launchTs: 1677690149000,
		oracleSource: OracleSource.PYTH_1M,
	},
	{
		fullName: 'Polygon',
		category: ['L2', 'Infra'],
		symbol: 'MATIC-PERP',
		baseAssetSymbol: 'MATIC',
		marketIndex: 5,
		oracle: new PublicKey('7KVswB9vkCgeM3SHP7aGDijvdRAHK8P5wi9JXViCrtYh'),
		launchTs: 1677690149000, //todo
		oracleSource: OracleSource.PYTH,
	},
];

export const PerpMarkets: { [key in DriftEnv]: PerpMarketConfig[] } = {
	devnet: DevnetPerpMarkets,
	'mainnet-beta': MainnetPerpMarkets,
};
