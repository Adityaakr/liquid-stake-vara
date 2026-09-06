import type { VaultAsset } from '@/domain/protocol';

export type NetworkId = 'mainnet';

/** Anything that can be deposited for a receipt token. */
export type DepositAsset = 'VARA' | VaultAsset;

export type VaultStats = {
  /** receipt -> asset exchange rate, scaled 1e9 (assets per share). */
  rate: bigint;
  apyBps: bigint;
  instantFeeBps: bigint;
  unbondSecs: number;
  minDeposit: bigint;
  totalShares: bigint;
  totalAssets: bigint;
  /** Underlying the vault physically holds (deposits and reserve). */
  holdings: bigint;
  tvlUsd: number;
  paused: boolean;
};

export type ProtocolStats = {
  /** kVARA -> VARA exchange rate, scaled 1e9 */
  rate: bigint;
  stakeApyBps: bigint;
  vaults: Record<VaultAsset, VaultStats>;
  tvlUsd: number;
  totalStakedVara: bigint;
  bufferBps: bigint;
  era: number;
  eraEndsAt: number;
  /** last three eras' rates for the compound timeline */
  rateHistory: { era: number; rate: bigint }[];
  varaPriceUsd: number;
  /** ms timestamp the rates above were computed at; the UI projects them forward at the APY. */
  at: number;
  /** Latest block seen on the node, when the adapter talks to a chain. */
  blockNumber?: number;
};

export type Balances = {
  VARA: bigint;
  kVARA: bigint;
  USDT: bigint;
  USDC: bigint;
  kUSDT: bigint;
  kUSDC: bigint;
  /** What the address put in, per deposit asset, when the adapter can track it (simulation). */
  principal?: Record<DepositAsset, bigint>;
};

export type UnbondEntry = {
  id: string;
  asset: DepositAsset;
  /** Amount of the underlying asset locked for this entry, in base units. */
  amount: bigint;
  startedAt: number;
  claimableAt: number;
};

export type FaucetInfo = {
  amount: bigint;
  cooldownSecs: number;
  /** ms timestamp after which the address may claim again; 0 when never claimed. */
  nextClaimAt: number;
};

export type TxResult = { hash: string; blockNumber?: number };

export type TxStage = 'idle' | 'broadcast' | 'finalized' | 'error';

export type Account = { address: string; name?: string; source: string };

export class ChainError extends Error {
  constructor(message: string, readonly code: 'NO_WALLET' | 'REJECTED' | 'INSUFFICIENT' | 'NOT_DEPLOYED' | 'RPC' | 'PROGRAM' | 'UNKNOWN' = 'UNKNOWN') {
    super(message);
    this.name = 'ChainError';
  }
}

/**
 * Everything the UI needs from the protocol. The mock implements the kit semantics exactly;
 * the Gear implementation talks to the Vale Protocol programs on Vara.
 */
export interface StakingAdapter {
  readonly kind: 'mock' | 'gear';
  readonly network: NetworkId;
  /** true when the whole protocol is simulated in the browser */
  readonly simulated: boolean;
  /** true when the vault programs are configured and reachable */
  readonly deployed: boolean;
  /** true when native VARA staking can be used (false on mainnet until the validator integration ships) */
  readonly stakingLive: boolean;
  getStats(): Promise<ProtocolStats>;
  getBalances(address: string): Promise<Balances>;
  getUnbonding(address: string): Promise<UnbondEntry[]>;
  getFaucet(address: string): Promise<Record<VaultAsset, FaucetInfo>>;
  // native VARA staking
  stake(address: string, vara: bigint): Promise<TxResult>;
  unstakeInstant(address: string, kvara: bigint): Promise<TxResult>;
  unstakeNative(address: string, kvara: bigint): Promise<TxResult>;
  // vaults
  claimFaucet(address: string, asset: VaultAsset): Promise<TxResult>;
  depositVault(address: string, asset: VaultAsset, amount: bigint): Promise<TxResult & { shares?: bigint }>;
  redeemVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult & { net?: bigint; fee?: bigint }>;
  unbondVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult & { amount?: bigint; claimableAt?: number }>;
  /** Claim a matured unbond entry of any asset. */
  claimUnbond(address: string, asset: DepositAsset, id: string): Promise<TxResult>;
  /** Local dev chains only: send the address some VARA for fees from a dev account. */
  devFund?: (address: string) => Promise<TxResult>;
  /** Subscribe to state changes (new blocks or simulated era ticks). Returns unsubscribe. */
  subscribe(cb: () => void): () => void;
  /** Release sockets and listeners. Called when the adapter is replaced. */
  dispose(): Promise<void>;
}
