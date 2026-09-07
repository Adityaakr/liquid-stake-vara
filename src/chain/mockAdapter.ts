import { BPS, INSTANT_UNSTAKE_FEE_BPS, ONE_STABLE, ONE_VARA, RATE_SCALE, UNBONDING_MS, VAULT_ASSETS, type VaultAsset } from '@/domain/protocol';
import { assetsToShares, instantUnstakeOut, nativeUnstakeOut, sharesToAssets, varaToKVara } from '@/domain/math';
import { STABLE_APY_BPS as VAULT_APY, STABLE_TVL_USD as VAULT_TVL, STAKED_VARA, T0, VARA_APY_BPS, VARA_PRICE_USD, elapsedMs, stablePriceAt, varaEra, varaRateAt, varaRateHistory } from './varaPool';
import { ChainError, type Balances, type DepositAsset, type FaucetInfo, type NetworkId, type ProtocolStats, type StakingAdapter, type TxResult, type UnbondEntry, type VaultStats } from './types';

type StoredUnbond = Omit<UnbondEntry, 'amount'> & { amount: string };
type StoredBalances = Record<Exclude<keyof Balances, 'principal'>, string> & { principal?: Record<DepositAsset, string> };
type Persisted = { balances: Record<string, StoredBalances>; unbonding: Record<string, StoredUnbond[]>; faucet: Record<string, Partial<Record<VaultAsset, number>>> };

const KEY = 'vale.mock.v3';

/**
 * The simulation's clock starts at T0 (see varaPool.ts): there the rates are the published ones
 * (kVARA 1.0482 at era 4,182, kUSDT 1.18, kUSDC 1.15). From then on every rate accrues
 * continuously at its APY, so a receipt is visibly worth more every second it is held.
 */
export { T0 };
const APY_BPS = VARA_APY_BPS;

const VAULT_UNBOND_SECS = 7 * 86_400;
const FAUCET_AMOUNT = 1_000n * ONE_STABLE;
const FAUCET_COOLDOWN_SECS = 24 * 3600;

const DEFAULT_BALANCES: Balances = {
  VARA: 1240n * ONE_VARA + (52n * ONE_VARA) / 100n,
  kVARA: 0n,
  USDT: 500n * ONE_STABLE,
  USDC: 250n * ONE_STABLE,
  kUSDT: 0n,
  kUSDC: 0n,
  principal: { VARA: 0n, USDT: 0n, USDC: 0n },
};

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const hash = () => '0x' + Array.from(crypto.getRandomValues(new Uint8Array(32)), (b) => b.toString(16).padStart(2, '0')).join('');

function load(): Persisted {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) { const p = JSON.parse(raw) as Persisted; return { balances: p.balances ?? {}, unbonding: p.unbonding ?? {}, faucet: p.faucet ?? {} }; }
  } catch { /* fresh state */ }
  return { balances: {}, unbonding: {}, faucet: {} };
}

/**
 * Deterministic in-memory protocol. Persists per address in localStorage so a refresh keeps
 * positions. Latency is simulated so every pending state in the UI is exercised. Tests inject a
 * fixed clock through `now` so the kit numbers stay exact.
 */
export class MockAdapter implements StakingAdapter {
  readonly kind = 'mock' as const;
  readonly simulated = true;
  readonly deployed = true;
  readonly stakingLive = true;
  private state: Persisted;
  private listeners = new Set<() => void>();
  private latency: number;
  private now: () => number;

  constructor(readonly network: NetworkId = 'mainnet', opts: { latencyMs?: number; storage?: boolean; now?: () => number } = {}) {
    this.latency = opts.latencyMs ?? 900;
    this.now = opts.now ?? (() => Date.now());
    this.state = opts.storage === false ? { balances: {}, unbonding: {}, faucet: {} } : load();
    this.persist = opts.storage === false ? () => {} : this.persist;
  }

  private persist() {
    try { localStorage.setItem(KEY, JSON.stringify(this.state)); } catch (e) { console.warn('mock persist failed', e); }
  }
  private emit() { this.listeners.forEach((l) => l()); }

  rateAt(now = this.now()): bigint {
    return varaRateAt(now);
  }

  sharePriceAt(asset: VaultAsset, now = this.now()): bigint {
    return stablePriceAt(asset, now);
  }

  private currentEra(now = this.now()): { era: number; endsAt: number } {
    return varaEra(now);
  }

  private vaultStats(asset: VaultAsset, now: number): VaultStats {
    const rate = this.sharePriceAt(asset, now);
    const totalAssets = BigInt(Math.round(VAULT_TVL[asset])) * ONE_STABLE;
    return {
      rate,
      apyBps: VAULT_APY[asset],
      instantFeeBps: INSTANT_UNSTAKE_FEE_BPS,
      unbondSecs: VAULT_UNBOND_SECS,
      minDeposit: 1_000n,
      totalShares: assetsToShares(totalAssets, rate),
      totalAssets,
      holdings: totalAssets,
      tvlUsd: VAULT_TVL[asset],
      paused: false,
    };
  }

  async getStats(): Promise<ProtocolStats> {
    const now = this.now();
    const { era, endsAt } = this.currentEra(now);
    return {
      rate: this.rateAt(now),
      stakeApyBps: APY_BPS,
      stakeFeeBps: INSTANT_UNSTAKE_FEE_BPS,
      stakeUnbondSecs: UNBONDING_MS / 1000,
      vaults: { USDT: this.vaultStats('USDT', now), USDC: this.vaultStats('USDC', now) },
      tvlUsd: (Number(STAKED_VARA) / Number(ONE_VARA)) * VARA_PRICE_USD + VAULT_TVL.USDT + VAULT_TVL.USDC,
      totalStakedVara: STAKED_VARA,
      bufferBps: 720n,
      era,
      eraEndsAt: endsAt,
      rateHistory: varaRateHistory(now),
      varaPriceUsd: VARA_PRICE_USD,
      at: now,
    };
  }

  private bal(address: string): Balances & { principal: Record<DepositAsset, bigint> } {
    const s = this.state.balances[address];
    if (!s) return { ...DEFAULT_BALANCES, principal: { ...DEFAULT_BALANCES.principal! } };
    const { principal, ...rest } = s;
    const out = Object.fromEntries(Object.entries(rest).map(([k, v]) => [k, BigInt(v)])) as Omit<Balances, 'principal'>;
    return { ...out, principal: { VARA: BigInt(principal?.VARA ?? '0'), USDT: BigInt(principal?.USDT ?? '0'), USDC: BigInt(principal?.USDC ?? '0') } };
  }
  private setBal(address: string, b: Balances & { principal: Record<DepositAsset, bigint> }) {
    const { principal, ...rest } = b;
    this.state.balances[address] = {
      ...(Object.fromEntries(Object.entries(rest).map(([k, v]) => [k, (v as bigint).toString()])) as Record<Exclude<keyof Balances, 'principal'>, string>),
      principal: { VARA: principal.VARA.toString(), USDT: principal.USDT.toString(), USDC: principal.USDC.toString() },
    };
    this.persist();
    this.emit();
  }

  /** Reduce the tracked principal in proportion to the receipts leaving the position. */
  private principalAfter(principal: bigint, receiptsOut: bigint, receiptsBefore: bigint): bigint {
    if (receiptsBefore <= 0n || receiptsOut >= receiptsBefore) return 0n;
    return principal - (principal * receiptsOut) / receiptsBefore;
  }

  async getBalances(address: string): Promise<Balances> {
    await wait(this.latency / 3);
    return this.bal(address);
  }

  async getUnbonding(address: string): Promise<UnbondEntry[]> {
    return (this.state.unbonding[address] ?? []).map((u) => ({ ...u, amount: BigInt(u.amount) }));
  }

  async getFaucet(address: string): Promise<Record<VaultAsset, FaucetInfo>> {
    const last = this.state.faucet[address] ?? {};
    return Object.fromEntries(VAULT_ASSETS.map((a) => [a, { amount: FAUCET_AMOUNT, cooldownSecs: FAUCET_COOLDOWN_SECS, nextClaimAt: last[a] ? last[a]! + FAUCET_COOLDOWN_SECS * 1000 : 0 }])) as Record<VaultAsset, FaucetInfo>;
  }

  /** Adapter-side guard so two overlapping writes for one address serialize instead of racing. */
  private locks = new Map<string, Promise<unknown>>();
  private async locked<T>(address: string, fn: () => Promise<T>): Promise<T> {
    const prev = this.locks.get(address) ?? Promise.resolve();
    const next = prev.then(fn, fn);
    this.locks.set(address, next.catch(() => undefined));
    return next;
  }

  private async tx(): Promise<TxResult> {
    await wait(this.latency);
    return { hash: hash(), blockNumber: 12_000_000 + Math.floor(elapsedMs(this.now()) / 3000) };
  }

  private pushUnbond(address: string, u: StoredUnbond) {
    const list = this.state.unbonding[address] ?? [];
    list.push(u);
    this.state.unbonding[address] = list;
  }

  stake(address: string, vara: bigint): Promise<TxResult> {
    return this.locked(address, async () => {
      if (vara <= 0n) throw new ChainError('Amount must be positive', 'INSUFFICIENT');
      if (vara > this.bal(address).VARA) throw new ChainError('Insufficient VARA balance', 'INSUFFICIENT');
      const rate = this.rateAt();
      const r = await this.tx();
      const b = this.bal(address);
      this.setBal(address, { ...b, VARA: b.VARA - vara, kVARA: b.kVARA + varaToKVara(vara, rate), principal: { ...b.principal, VARA: b.principal.VARA + vara } });
      return r;
    });
  }

  unstakeInstant(address: string, kvara: bigint): Promise<TxResult> {
    return this.locked(address, async () => {
      if (kvara <= 0n || kvara > this.bal(address).kVARA) throw new ChainError('Insufficient kVARA balance', 'INSUFFICIENT');
      const { net } = instantUnstakeOut(kvara, this.rateAt());
      const r = await this.tx();
      const b = this.bal(address);
      this.setBal(address, { ...b, kVARA: b.kVARA - kvara, VARA: b.VARA + net, principal: { ...b.principal, VARA: this.principalAfter(b.principal.VARA, kvara, b.kVARA) } });
      return r;
    });
  }

  unstakeNative(address: string, kvara: bigint): Promise<TxResult> {
    return this.locked(address, async () => {
      if (kvara <= 0n || kvara > this.bal(address).kVARA) throw new ChainError('Insufficient kVARA balance', 'INSUFFICIENT');
      const out = nativeUnstakeOut(kvara, this.rateAt());
      const r = await this.tx();
      const now = this.now();
      this.pushUnbond(address, { id: r.hash.slice(0, 10), asset: 'VARA', amount: out.toString(), startedAt: now, claimableAt: now + UNBONDING_MS });
      const b = this.bal(address);
      this.setBal(address, { ...b, kVARA: b.kVARA - kvara, principal: { ...b.principal, VARA: this.principalAfter(b.principal.VARA, kvara, b.kVARA) } });
      return r;
    });
  }

  claimUnbond(address: string, asset: DepositAsset, id: string): Promise<TxResult> {
    return this.locked(address, async () => {
      const list = this.state.unbonding[address] ?? [];
      const entry = list.find((u) => u.id === id && u.asset === asset);
      if (!entry) throw new ChainError('Unbond entry not found', 'UNKNOWN');
      if (this.now() < entry.claimableAt) throw new ChainError('Unbonding period has not ended', 'UNKNOWN');
      const r = await this.tx();
      this.state.unbonding[address] = list.filter((u) => u.id !== id);
      const b = this.bal(address);
      this.setBal(address, { ...b, [asset]: b[asset] + BigInt(entry.amount) });
      return r;
    });
  }

  claimFaucet(address: string, asset: VaultAsset): Promise<TxResult> {
    return this.locked(address, async () => {
      const f = (await this.getFaucet(address))[asset];
      if (this.now() < f.nextClaimAt) throw new ChainError('The faucet is cooling down for this address', 'PROGRAM');
      const r = await this.tx();
      this.state.faucet[address] = { ...(this.state.faucet[address] ?? {}), [asset]: this.now() };
      const b = this.bal(address);
      this.setBal(address, { ...b, [asset]: b[asset] + f.amount });
      return r;
    });
  }

  depositVault(address: string, asset: VaultAsset, amount: bigint): Promise<TxResult & { shares?: bigint }> {
    return this.locked(address, async () => {
      const rec = `k${asset}` as const;
      if (amount <= 0n || amount > this.bal(address)[asset]) throw new ChainError(`Insufficient ${asset} balance`, 'INSUFFICIENT');
      const shares = assetsToShares(amount, this.sharePriceAt(asset));
      const r = await this.tx();
      const b = this.bal(address);
      this.setBal(address, { ...b, [asset]: b[asset] - amount, [rec]: b[rec] + shares, principal: { ...b.principal, [asset]: b.principal[asset] + amount } });
      return { ...r, shares };
    });
  }

  redeemVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult & { net?: bigint; fee?: bigint }> {
    return this.locked(address, async () => {
      const rec = `k${asset}` as const;
      if (shares <= 0n || shares > this.bal(address)[rec]) throw new ChainError(`Insufficient ${rec} balance`, 'INSUFFICIENT');
      const gross = sharesToAssets(shares, this.sharePriceAt(asset));
      const fee = (gross * INSTANT_UNSTAKE_FEE_BPS) / BPS;
      const r = await this.tx();
      const b = this.bal(address);
      this.setBal(address, { ...b, [rec]: b[rec] - shares, [asset]: b[asset] + gross - fee, principal: { ...b.principal, [asset]: this.principalAfter(b.principal[asset], shares, b[rec]) } });
      return { ...r, net: gross - fee, fee };
    });
  }

  unbondVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult & { amount?: bigint; claimableAt?: number }> {
    return this.locked(address, async () => {
      const rec = `k${asset}` as const;
      if (shares <= 0n || shares > this.bal(address)[rec]) throw new ChainError(`Insufficient ${rec} balance`, 'INSUFFICIENT');
      const assets = sharesToAssets(shares, this.sharePriceAt(asset));
      const r = await this.tx();
      const now = this.now();
      const claimableAt = now + VAULT_UNBOND_SECS * 1000;
      this.pushUnbond(address, { id: r.hash.slice(0, 10), asset, amount: assets.toString(), startedAt: now, claimableAt });
      const b = this.bal(address);
      this.setBal(address, { ...b, [rec]: b[rec] - shares, principal: { ...b.principal, [asset]: this.principalAfter(b.principal[asset], shares, b[rec]) } });
      return { ...r, amount: assets, claimableAt };
    });
  }

  subscribe(cb: () => void) {
    this.listeners.add(cb);
    return () => { this.listeners.delete(cb); };
  }

  async dispose() { this.listeners.clear(); }

  /** test helper: fast-forward unbonding entries so claims can be exercised */
  _debugFastForward(address: string) {
    for (const u of this.state.unbonding[address] ?? []) u.claimableAt = this.now() - 1;
    this.persist();
    this.emit();
  }
}

export { RATE_SCALE };
