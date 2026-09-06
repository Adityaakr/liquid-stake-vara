import { BPS, INSTANT_UNSTAKE_FEE_BPS, ONE_STABLE, ONE_VARA, RATE_SCALE, UNBONDING_MS, type VaultAsset } from '@/domain/protocol';
import { accrueRate, assetsToShares, instantUnstakeOut, nativeUnstakeOut, sharesToAssets, varaToKVara } from '@/domain/math';
import { parseRate } from '@/domain/format';
import { ChainError, type Balances, type DepositAsset, type NetworkId, type ProtocolStats, type StakingAdapter, type TxResult, type UnbondEntry } from './types';

type StoredUnbond = Omit<UnbondEntry, 'amountVara'> & { amountVara: string };
type StoredBalances = Record<Exclude<keyof Balances, 'principal'>, string> & { principal?: Record<DepositAsset, string> };
type Persisted = { balances: Record<string, StoredBalances>; unbonding: Record<string, StoredUnbond[]> };

const KEY = 'vale.mock.v2';
const ERA_MS = 12 * 60 * 60 * 1000;

/**
 * The simulation's clock starts here: at T0 the numbers are exactly the kit's (rate 1.0482 at
 * era 4,182, share prices 1.0261 and 1.0193). From then on every rate accrues continuously at
 * its APY, so a receipt is visibly worth more every second it is held.
 */
export const T0 = Date.UTC(2026, 8, 1);
const BASE_ERA = 4182;
const BASE_RATE = parseRate('1.0482');
const APY_BPS = 1420n;

const VAULT_APY: Record<VaultAsset, bigint> = { USDT: 840n, USDC: 790n };
const VAULT_BASE_PRICE: Record<VaultAsset, bigint> = { USDT: parseRate('1.0261'), USDC: parseRate('1.0193') };
/** Pool sizes: kVARA 918.27M VARA at $0.0004258 = $391K, kUSDT $577K, kUSDC $230K, combined $1.198M. */
const VAULT_TVL: Record<VaultAsset, number> = { USDT: 577_000, USDC: 230_000 };
const STAKED_VARA = 918_270_000n * ONE_VARA;
const VARA_PRICE_USD = 0.0004258;
const VAULT_UTIL: Record<VaultAsset, bigint> = { USDT: 7200n, USDC: 6400n };

const DEFAULT_BALANCES: Balances = {
  VARA: 1240n * ONE_VARA + (52n * ONE_VARA) / 100n,
  kVARA: 0n,
  wUSDT: 500n * ONE_STABLE,
  wUSDC: 250n * ONE_STABLE,
  kUSDT: 0n,
  kUSDC: 0n,
  principal: { VARA: 0n, USDT: 0n, USDC: 0n },
};

const wait = (ms: number) => new Promise((r) => setTimeout(r, ms));
const hash = () => '0x' + Array.from(crypto.getRandomValues(new Uint8Array(32)), (b) => b.toString(16).padStart(2, '0')).join('');

function load(): Persisted {
  try {
    const raw = localStorage.getItem(KEY);
    if (raw) return JSON.parse(raw) as Persisted;
  } catch { /* fresh state */ }
  return { balances: {}, unbonding: {} };
}

/**
 * Deterministic in-memory protocol. Persists per address in localStorage so a refresh keeps
 * positions. Latency is simulated so every pending state in the UI is exercised. Tests inject a
 * fixed clock through `now` so the kit numbers stay exact.
 */
export class MockAdapter implements StakingAdapter {
  readonly kind = 'mock' as const;
  readonly simulated = true;
  private state: Persisted;
  private listeners = new Set<() => void>();
  private latency: number;
  private now: () => number;

  constructor(readonly network: NetworkId = 'mainnet', opts: { latencyMs?: number; storage?: boolean; now?: () => number } = {}) {
    this.latency = opts.latencyMs ?? 900;
    this.now = opts.now ?? (() => Date.now());
    this.state = opts.storage === false ? { balances: {}, unbonding: {} } : load();
    this.persist = opts.storage === false ? () => {} : this.persist;
  }

  private persist() {
    try { localStorage.setItem(KEY, JSON.stringify(this.state)); } catch (e) { console.warn('mock persist failed', e); }
  }
  private emit() { this.listeners.forEach((l) => l()); }

  /** Whole seconds since T0, never negative, so rates are stable within a second. */
  private elapsedMs(now = this.now()): number {
    return Math.max(0, Math.floor((now - T0) / 1000) * 1000);
  }

  rateAt(now = this.now()): bigint {
    return accrueRate(BASE_RATE, APY_BPS, this.elapsedMs(now));
  }

  sharePriceAt(asset: VaultAsset, now = this.now()): bigint {
    return accrueRate(VAULT_BASE_PRICE[asset], VAULT_APY[asset], this.elapsedMs(now));
  }

  private currentEra(now = this.now()): { era: number; endsAt: number } {
    const elapsed = this.elapsedMs(now);
    const era = BASE_ERA + Math.floor(elapsed / ERA_MS);
    const endsAt = T0 + (era - BASE_ERA + 1) * ERA_MS;
    return { era, endsAt };
  }

  async getStats(): Promise<ProtocolStats> {
    const now = this.now();
    const { era, endsAt } = this.currentEra(now);
    const rate = this.rateAt(now);
    const eraRate = (e: number) => accrueRate(BASE_RATE, APY_BPS, Math.max(0, (e - BASE_ERA) * ERA_MS));
    return {
      rate,
      stakeApyBps: APY_BPS,
      vaultApyBps: { ...VAULT_APY },
      vaultSharePrice: { USDT: this.sharePriceAt('USDT', now), USDC: this.sharePriceAt('USDC', now) },
      vaultTvlUsd: { ...VAULT_TVL },
      vaultUtilizationBps: { ...VAULT_UTIL },
      tvlUsd: (Number(STAKED_VARA) / Number(ONE_VARA)) * VARA_PRICE_USD + VAULT_TVL.USDT + VAULT_TVL.USDC,
      totalStakedVara: STAKED_VARA,
      bufferBps: 720n,
      era,
      eraEndsAt: endsAt,
      rateHistory: [era - 2, era - 1, era].map((e) => ({ era: e, rate: e === era ? rate : eraRate(e) })),
      varaPriceUsd: VARA_PRICE_USD,
      at: now,
    };
  }

  private bal(address: string): Balances {
    const s = this.state.balances[address];
    if (!s) return { ...DEFAULT_BALANCES, principal: { ...DEFAULT_BALANCES.principal } };
    const { principal, ...rest } = s;
    const out = Object.fromEntries(Object.entries(rest).map(([k, v]) => [k, BigInt(v)])) as Omit<Balances, 'principal'>;
    return { ...out, principal: { VARA: BigInt(principal?.VARA ?? '0'), USDT: BigInt(principal?.USDT ?? '0'), USDC: BigInt(principal?.USDC ?? '0') } };
  }
  private setBal(address: string, b: Balances) {
    const { principal, ...rest } = b;
    this.state.balances[address] = {
      ...(Object.fromEntries(Object.entries(rest).map(([k, v]) => [k, v.toString()])) as Record<Exclude<keyof Balances, 'principal'>, string>),
      principal: { VARA: principal.VARA.toString(), USDT: principal.USDT.toString(), USDC: principal.USDC.toString() },
    };
    this.persist();
    this.emit();
  }

  /** Reduce the tracked principal in proportion to the receipts leaving the position. */
  private principalAfter(b: Balances, asset: DepositAsset, receiptsOut: bigint, receiptsBefore: bigint): bigint {
    if (receiptsBefore <= 0n || receiptsOut >= receiptsBefore) return 0n;
    return b.principal[asset] - (b.principal[asset] * receiptsOut) / receiptsBefore;
  }

  async getBalances(address: string): Promise<Balances> {
    await wait(this.latency / 3);
    return this.bal(address);
  }

  async getUnbonding(address: string): Promise<UnbondEntry[]> {
    return (this.state.unbonding[address] ?? []).map((u) => ({ ...u, amountVara: BigInt(u.amountVara) }));
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
    return { hash: hash(), blockNumber: 12_000_000 + Math.floor(this.elapsedMs() / 3000) };
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
      this.setBal(address, { ...b, kVARA: b.kVARA - kvara, VARA: b.VARA + net, principal: { ...b.principal, VARA: this.principalAfter(b, 'VARA', kvara, b.kVARA) } });
      return r;
    });
  }

  unstakeNative(address: string, kvara: bigint): Promise<TxResult> {
    return this.locked(address, async () => {
      if (kvara <= 0n || kvara > this.bal(address).kVARA) throw new ChainError('Insufficient kVARA balance', 'INSUFFICIENT');
      const out = nativeUnstakeOut(kvara, this.rateAt());
      const r = await this.tx();
      const now = this.now();
      const list = this.state.unbonding[address] ?? [];
      list.push({ id: r.hash.slice(0, 10), amountVara: out.toString(), startedAt: now, claimableAt: now + UNBONDING_MS });
      this.state.unbonding[address] = list;
      const b = this.bal(address);
      this.setBal(address, { ...b, kVARA: b.kVARA - kvara, principal: { ...b.principal, VARA: this.principalAfter(b, 'VARA', kvara, b.kVARA) } });
      return r;
    });
  }

  claimUnbonded(address: string, id: string): Promise<TxResult> {
    return this.locked(address, async () => {
      const list = this.state.unbonding[address] ?? [];
      const entry = list.find((u) => u.id === id);
      if (!entry) throw new ChainError('Unbond entry not found', 'UNKNOWN');
      if (this.now() < entry.claimableAt) throw new ChainError('Unbonding period has not ended', 'UNKNOWN');
      const r = await this.tx();
      this.state.unbonding[address] = (this.state.unbonding[address] ?? []).filter((u) => u.id !== id);
      const b = this.bal(address);
      this.setBal(address, { ...b, VARA: b.VARA + BigInt(entry.amountVara) });
      return r;
    });
  }

  depositVault(address: string, asset: VaultAsset, amount: bigint): Promise<TxResult> {
    return this.locked(address, async () => {
      const dep = `w${asset}` as const;
      const rec = `k${asset}` as const;
      if (amount <= 0n || amount > this.bal(address)[dep]) throw new ChainError(`Insufficient ${dep} balance`, 'INSUFFICIENT');
      const price = this.sharePriceAt(asset);
      const r = await this.tx();
      const b = this.bal(address);
      this.setBal(address, { ...b, [dep]: b[dep] - amount, [rec]: b[rec] + assetsToShares(amount, price), principal: { ...b.principal, [asset]: b.principal[asset] + amount } });
      return r;
    });
  }

  redeemVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult> {
    return this.locked(address, async () => {
      const dep = `w${asset}` as const;
      const rec = `k${asset}` as const;
      if (shares <= 0n || shares > this.bal(address)[rec]) throw new ChainError(`Insufficient ${rec} balance`, 'INSUFFICIENT');
      const gross = sharesToAssets(shares, this.sharePriceAt(asset));
      const net = gross - (gross * INSTANT_UNSTAKE_FEE_BPS) / BPS;
      const r = await this.tx();
      const b = this.bal(address);
      this.setBal(address, { ...b, [rec]: b[rec] - shares, [dep]: b[dep] + net, principal: { ...b.principal, [asset]: this.principalAfter(b, asset, shares, b[rec]) } });
      return r;
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

export { RATE_SCALE, BPS };
