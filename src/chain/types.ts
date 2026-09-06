import type { VaultAsset } from '@/domain/protocol';

export type NetworkId = 'mainnet';

/** Anything that can be deposited for a receipt token. */
export type DepositAsset = 'VARA' | VaultAsset;

export type ProtocolStats = {
  /** kVARA -> VARA exchange rate, scaled 1e9 */
  rate: bigint;
  stakeApyBps: bigint;
  vaultApyBps: Record<VaultAsset, bigint>;
  vaultSharePrice: Record<VaultAsset, bigint>;
  vaultTvlUsd: Record<VaultAsset, number>;
  vaultUtilizationBps: Record<VaultAsset, bigint>;
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
};

export type Balances = {
  VARA: bigint;
  kVARA: bigint;
  wUSDT: bigint;
  wUSDC: bigint;
  kUSDT: bigint;
  kUSDC: bigint;
  /** What the address put in, per deposit asset, so the UI can show what the receipts have earned. */
  principal: Record<DepositAsset, bigint>;
};

export type UnbondEntry = { id: string; amountVara: bigint; startedAt: number; claimableAt: number };

export type TxResult = { hash: string; blockNumber?: number };

export type TxStage = 'idle' | 'broadcast' | 'finalized' | 'error';

export type Account = { address: string; name?: string; source: string };

export class ChainError extends Error {
  constructor(message: string, readonly code: 'NO_WALLET' | 'REJECTED' | 'INSUFFICIENT' | 'NOT_DEPLOYED' | 'RPC' | 'UNKNOWN' = 'UNKNOWN') {
    super(message);
    this.name = 'ChainError';
  }
}

/**
 * Everything the UI needs from the protocol. The mock implements the kit semantics exactly;
 * the Gear implementation talks to Vara and delegates to a Sails program once one is configured.
 */
export interface StakingAdapter {
  readonly kind: 'mock' | 'gear';
  readonly network: NetworkId;
  /** true when program writes are simulated rather than sent on chain */
  readonly simulated: boolean;
  getStats(): Promise<ProtocolStats>;
  getBalances(address: string): Promise<Balances>;
  getUnbonding(address: string): Promise<UnbondEntry[]>;
  stake(address: string, vara: bigint): Promise<TxResult>;
  unstakeInstant(address: string, kvara: bigint): Promise<TxResult>;
  unstakeNative(address: string, kvara: bigint): Promise<TxResult>;
  claimUnbonded(address: string, id: string): Promise<TxResult>;
  depositVault(address: string, asset: VaultAsset, amount: bigint): Promise<TxResult>;
  /** Instant exit from a stable vault: burn shares, receive the asset minus the instant fee. */
  redeemVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult>;
  /** Subscribe to stat changes (era ticks). Returns unsubscribe. */
  subscribe(cb: () => void): () => void;
  /** Release sockets and listeners. Called when the adapter is replaced. */
  dispose(): Promise<void>;
}
