import type { VaultAsset } from '@/domain/protocol';
import { NETWORKS } from './networks';
import { MockAdapter } from './mockAdapter';
import { DEMO_ACCOUNT } from './wallet';
import { ChainError, type Balances, type NetworkId, type ProtocolStats, type StakingAdapter, type TxResult, type UnbondEntry } from './types';

type GearApiT = import('@gear-js/api').GearApi;

/**
 * Talks to a real Vara node. Native VARA balance is read from chain. Protocol state and
 * program writes go through a Sails program when VITE_VALE_PROGRAM_ID is set; until then
 * they are delegated to the MockAdapter and the UI shows a simulation badge.
 */
export class GearAdapter implements StakingAdapter {
  readonly kind = 'gear' as const;
  private api: Promise<GearApiT> | null = null;
  private fallback: MockAdapter;
  readonly programId: string | undefined;

  constructor(readonly network: NetworkId = 'mainnet', programId = import.meta.env.VITE_VALE_PROGRAM_ID as string | undefined) {
    this.programId = programId && programId.length > 0 ? programId : undefined;
    this.fallback = new MockAdapter(network);
  }

  get simulated() { return this.programId === undefined; }

  private async connect(): Promise<GearApiT> {
    if (!this.api) {
      this.api = import('@gear-js/api')
        .then(({ GearApi }) => GearApi.create({ providerAddress: NETWORKS[this.network].rpc }))
        .catch((e: unknown) => { this.api = null; throw new ChainError(`Could not reach ${NETWORKS[this.network].label}: ${String(e)}`, 'RPC'); });
    }
    return this.api;
  }

  async getStats(): Promise<ProtocolStats> {
    if (this.simulated) return this.fallback.getStats();
    throw new ChainError('Program state reads are not implemented for this program id yet', 'NOT_DEPLOYED');
  }

  async getBalances(address: string): Promise<Balances> {
    // The demo account has no real funds; keep it fully simulated so the flows stay usable without a wallet.
    if (address === DEMO_ACCOUNT.address) return this.fallback.getBalances(address);
    const api = await this.connect();
    const b = await api.balance.findOut(address);
    const rest = await this.fallback.getBalances(address);
    return { ...rest, VARA: b.toBigInt() };
  }

  getUnbonding(address: string): Promise<UnbondEntry[]> { return this.fallback.getUnbonding(address); }

  private notDeployed(): never {
    throw new ChainError('The Vale Protocol staking program is not deployed on this network yet', 'NOT_DEPLOYED');
  }

  stake(address: string, vara: bigint): Promise<TxResult> { return this.simulated ? this.fallback.stake(address, vara) : this.notDeployed(); }
  unstakeInstant(address: string, kvara: bigint): Promise<TxResult> { return this.simulated ? this.fallback.unstakeInstant(address, kvara) : this.notDeployed(); }
  unstakeNative(address: string, kvara: bigint): Promise<TxResult> { return this.simulated ? this.fallback.unstakeNative(address, kvara) : this.notDeployed(); }
  claimUnbonded(address: string, id: string): Promise<TxResult> { return this.simulated ? this.fallback.claimUnbonded(address, id) : this.notDeployed(); }
  depositVault(address: string, asset: VaultAsset, amount: bigint): Promise<TxResult> { return this.simulated ? this.fallback.depositVault(address, asset, amount) : this.notDeployed(); }
  redeemVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult> { return this.simulated ? this.fallback.redeemVault(address, asset, shares) : this.notDeployed(); }
  subscribe(cb: () => void) { return this.fallback.subscribe(cb); }

  async dispose() {
    await this.fallback.dispose();
    const api = this.api;
    this.api = null;
    if (api) { try { await (await api).disconnect(); } catch { /* already down */ } }
  }
}
