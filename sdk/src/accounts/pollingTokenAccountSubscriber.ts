import {
	AccountAndSlot,
	NotSubscribedError,
	TokenAccountEvents,
	TokenAccountSubscriber,
} from './types';
import { Program } from '@project-serum/anchor';
import StrictEventEmitter from 'strict-event-emitter-types';
import { EventEmitter } from 'events';
import { PublicKey } from '@solana/web3.js';
import { BulkAccountLoader } from './bulkAccountLoader';
import { AccountInfo } from '@solana/spl-token';
import { parseTokenAccount } from '../token';

export class PollingTokenAccountSubscriber implements TokenAccountSubscriber {
	isSubscribed: boolean;
	program: Program;
	eventEmitter: StrictEventEmitter<EventEmitter, TokenAccountEvents>;
	publicKey: PublicKey;

	accountLoader: BulkAccountLoader;
	callbackId?: string;
	errorCallbackId?: string;

	tokenAccountAndSlot?: AccountAndSlot<AccountInfo>;

	public constructor(publicKey: PublicKey, accountLoader: BulkAccountLoader) {
		this.isSubscribed = false;
		this.publicKey = publicKey;
		this.accountLoader = accountLoader;
		this.eventEmitter = new EventEmitter();
	}

	async subscribe(): Promise<boolean> {
		if (this.isSubscribed) {
			return true;
		}

		this.addToAccountLoader();
		let subscriptionSucceeded = false;
		let retries = 0;
		while (!subscriptionSucceeded && retries < 5) {
			await this.fetch();
			subscriptionSucceeded = this.didSubscriptionSucceed();
			retries++;
		}

		if (subscriptionSucceeded) {
			this.eventEmitter.emit('update');
		}

		this.isSubscribed = subscriptionSucceeded;
		return subscriptionSucceeded;
	}

	addToAccountLoader(): void {
		if (this.callbackId) {
			return;
		}

		this.callbackId = this.accountLoader.addAccount(
			this.publicKey,
			(buffer, slot: number) => {
				const tokenAccount = parseTokenAccount(buffer);
				this.tokenAccountAndSlot = { account: tokenAccount, slot };
				// @ts-ignore
				this.eventEmitter.emit('tokenAccountUpdate', tokenAccount);
				this.eventEmitter.emit('update');
			}
		);

		this.errorCallbackId = this.accountLoader.addErrorCallbacks((error) => {
			this.eventEmitter.emit('error', error);
		});
	}

	async fetch(): Promise<void> {
		await this.accountLoader.load();
		const { buffer, slot } = this.accountLoader.getBufferAndSlot(
			this.publicKey
		);
		this.tokenAccountAndSlot = { account: parseTokenAccount(buffer), slot };
	}

	async unsubscribe(): Promise<void> {
		if (!this.isSubscribed) {
			return;
		}

		this.accountLoader.removeAccount(this.publicKey, this.callbackId);
		this.callbackId = undefined;

		this.accountLoader.removeErrorCallbacks(this.errorCallbackId);
		this.errorCallbackId = undefined;

		this.isSubscribed = false;
	}

	assertIsSubscribed(): void {
		if (!this.isSubscribed) {
			throw new NotSubscribedError(
				'You must call `subscribe` before using this function'
			);
		}
	}

	public getTokenAccountAndSlot(): AccountAndSlot<AccountInfo> {
		this.assertIsSubscribed();
		return this.tokenAccountAndSlot;
	}

	didSubscriptionSucceed(): boolean {
		return !!this.tokenAccountAndSlot;
	}
}
