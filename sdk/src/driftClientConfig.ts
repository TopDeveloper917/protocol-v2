import { ConfirmOptions, Connection, PublicKey } from '@solana/web3.js';
import { IWallet } from './types';
import { OracleInfo } from './oracles/types';
import { BulkAccountLoader } from './accounts/bulkAccountLoader';
import { DriftEnv } from './config';

export type DriftClientConfig = {
	connection: Connection;
	wallet: IWallet;
	programID: PublicKey;
	accountSubscription?: DriftClientSubscriptionConfig;
	opts?: ConfirmOptions;
	txSenderConfig?: TxSenderConfig;
	subAccountIds?: number[];
	activeSubAccountId?: number;
	perpMarketIndexes?: number[];
	spotMarketIndexes?: number[];
	oracleInfos?: OracleInfo[];
	env?: DriftEnv;
	userStats?: boolean;
	authority?: PublicKey; // explicitly pass an authority if signer is delegate
};

export type DriftClientSubscriptionConfig =
	| {
			type: 'websocket';
	  }
	| {
			type: 'polling';
			accountLoader: BulkAccountLoader;
	  };

type TxSenderConfig = {
	type: 'retry';
	timeout?: number;
	retrySleep?: number;
	additionalConnections?: Connection[];
};
