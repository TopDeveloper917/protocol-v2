import {
	User,
	DriftClient,
	UserAccount,
	bulkPollingUserSubscribe,
	OrderRecord,
	UserSubscriptionConfig,
	WrappedEvent,
	DepositRecord,
	FundingPaymentRecord,
	LiquidationRecord,
	OrderActionRecord,
	SettlePnlRecord,
	NewUserRecord,
	LPRecord,
} from '..';
import { ProgramAccount } from '@project-serum/anchor';

import { PublicKey } from '@solana/web3.js';

export interface UserMapInterface {
	fetchAllUsers(): Promise<void>;
	addPubkey(userAccountPublicKey: PublicKey): Promise<void>;
	has(key: string): boolean;
	get(key: string): User | undefined;
	mustGet(key: string): Promise<User>;
	getUserAuthority(key: string): PublicKey | undefined;
	updateWithOrderRecord(record: OrderRecord): Promise<void>;
	values(): IterableIterator<User>;
}

export class UserMap implements UserMapInterface {
	private userMap = new Map<string, User>();
	private driftClient: DriftClient;
	private accountSubscription: UserSubscriptionConfig;

	constructor(
		driftClient: DriftClient,
		accountSubscription: UserSubscriptionConfig
	) {
		this.driftClient = driftClient;
		this.accountSubscription = accountSubscription;
	}

	public async fetchAllUsers() {
		const userArray: User[] = [];

		const programUserAccounts =
			(await this.driftClient.program.account.user.all()) as ProgramAccount<UserAccount>[];
		for (const programUserAccount of programUserAccounts) {
			if (this.userMap.has(programUserAccount.publicKey.toString())) {
				continue;
			}

			const user = new User({
				driftClient: this.driftClient,
				userAccountPublicKey: programUserAccount.publicKey,
				accountSubscription: this.accountSubscription,
			});
			userArray.push(user);
		}

		if (this.accountSubscription.type === 'polling') {
			await bulkPollingUserSubscribe(
				userArray,
				this.accountSubscription.accountLoader
			);
		} else {
			await Promise.all(userArray.map((user) => user.subscribe()));
		}

		for (const user of userArray) {
			this.userMap.set(user.getUserAccountPublicKey().toString(), user);
		}
	}

	public async addPubkey(userAccountPublicKey: PublicKey) {
		const user = new User({
			driftClient: this.driftClient,
			userAccountPublicKey,
			accountSubscription: this.accountSubscription,
		});
		await user.subscribe();
		this.userMap.set(userAccountPublicKey.toString(), user);
	}

	public has(key: string): boolean {
		return this.userMap.has(key);
	}

	/**
	 * gets the User for a particular userAccountPublicKey, if no User exists, undefined is returned
	 * @param key userAccountPublicKey to get User for
	 * @returns user User | undefined
	 */
	public get(key: string): User | undefined {
		return this.userMap.get(key);
	}

	/**
	 * gets the User for a particular userAccountPublicKey, if no User exists, new one is created
	 * @param key userAccountPublicKey to get User for
	 * @returns  User
	 */
	public async mustGet(key: string): Promise<User> {
		if (!this.has(key)) {
			await this.addPubkey(new PublicKey(key));
		}
		const user = this.userMap.get(key);
		await user.fetchAccounts();
		return user;
	}

	/**
	 * gets the Authority for a particular userAccountPublicKey, if no User exists, undefined is returned
	 * @param key userAccountPublicKey to get User for
	 * @returns authority PublicKey | undefined
	 */
	public getUserAuthority(key: string): PublicKey | undefined {
		const chUser = this.userMap.get(key);
		if (!chUser) {
			return undefined;
		}
		return chUser.getUserAccount().authority;
	}

	public async updateWithOrderRecord(record: OrderRecord) {
		if (!this.has(record.user.toString())) {
			await this.addPubkey(record.user);
		}
	}

	public async updateWithEventRecord(record: WrappedEvent<any>) {
		if (record.eventType === 'DepositRecord') {
			const depositRecord = record as DepositRecord;
			await this.mustGet(depositRecord.user.toString());
		} else if (record.eventType === 'FundingPaymentRecord') {
			const fundingPaymentRecord = record as FundingPaymentRecord;
			await this.mustGet(fundingPaymentRecord.user.toString());
		} else if (record.eventType === 'LiquidationRecord') {
			const liqRecord = record as LiquidationRecord;

			await this.mustGet(liqRecord.user.toString());
			await this.mustGet(liqRecord.liquidator.toString());
		} else if (record.eventType === 'OrderRecord') {
			const orderRecord = record as OrderRecord;
			await this.updateWithOrderRecord(orderRecord);
		} else if (record.eventType === 'OrderActionRecord') {
			const actionRecord = record as OrderActionRecord;

			if (actionRecord.taker) {
				await this.mustGet(actionRecord.taker.toString());
			}
			if (actionRecord.maker) {
				await this.mustGet(actionRecord.maker.toString());
			}
		} else if (record.eventType === 'SettlePnlRecord') {
			const settlePnlRecord = record as SettlePnlRecord;
			await this.mustGet(settlePnlRecord.user.toString());
		} else if (record.eventType === 'NewUserRecord') {
			const newUserRecord = record as NewUserRecord;
			await this.mustGet(newUserRecord.user.toString());
		} else if (record.eventType === 'LPRecord') {
			const lpRecord = record as LPRecord;
			await this.mustGet(lpRecord.user.toString());
		}
	}

	public values(): IterableIterator<User> {
		return this.userMap.values();
	}

	public size(): number {
		return this.userMap.size;
	}
}
