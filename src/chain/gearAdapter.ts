import type { HexString } from '@gear-js/api';
import { ONE_STABLE, RATE_SCALE, VAULT_ASSETS, type VaultAsset } from '@/domain/protocol';
import { NETWORKS } from './networks';
import { APP_NAME } from './wallet';
import { GAS, readPrograms, type ProgramSet } from './config';
import { ChainError, type Balances, type FaucetInfo, type NetworkId, type DepositAsset, type ProtocolStats, type StakingAdapter, type TxResult, type UnbondEntry, type VaultStats } from './types';
import type { DemoToken } from './idl/demo_token';
import type { Vault } from './idl/vault';

type GearApiT = import('@gear-js/api').GearApi;
type TxBuilder<T> = import('sails-js').TransactionBuilderWithHeader<T>;
type Outcome<T, E> = { ok: T } | { err: E };
type WithAccountArgs = Parameters<TxBuilder<unknown>['withAccount']>;

/** Who signs for an address: a browser extension signer (address + signer) or a keyring pair (scripts, tests). */
export type Signer = { account: WithAccountArgs[0]; options?: WithAccountArgs[1] };
export type SignerProvider = (address: string) => Promise<Signer>;

const RATE_1E18_TO_1E9 = 1_000_000_000n;
const viteEnv: Record<string, string | undefined> = (import.meta as unknown as { env?: Record<string, string | undefined> }).env ?? {};
const STABLE_PRICE_USD = 1;
const POLL_MS = 12_000;

async function extensionSigner(address: string): Promise<Signer> {
  const { web3Enable, web3FromAddress } = await import('@polkadot/extension-dapp');
  // A remembered account skips the connect dialog, so the extension may not be enabled for this page yet.
  const enabled = await web3Enable(APP_NAME);
  if (enabled.length === 0) throw new ChainError('No wallet extension allowed access to Vale Protocol. Open the extension and allow this site.', 'NO_WALLET');
  const injector = await web3FromAddress(address);
  return { account: address, options: { signer: injector.signer } };
}

/** Turn a program error enum (`{ Variant: payload }`) into a sentence. */
export function describeProgramError(err: unknown): string {
  if (err === null || typeof err !== 'object') return String(err);
  const [variant, payload] = Object.entries(err as Record<string, unknown>)[0] ?? ['Unknown', null];
  // sails-js decodes big integers as hex strings; show them as plain numbers.
  const show = (v: unknown) => (typeof v === 'string' && /^0x[0-9a-f]{10,}$/i.test(v) ? BigInt(v).toString() : String(v));
  const detail = typeof payload === 'string' ? payload : payload && typeof payload === 'object' ? Object.entries(payload).map(([k, v]) => `${k} ${show(v)}`).join(', ') : '';
  const text: Record<string, string> = {
    Paused: 'The program is paused',
    ZeroAmount: 'Amount must be more than zero',
    BelowMinimum: 'Amount is below the vault minimum',
    InsufficientShares: 'Not enough receipt tokens',
    InsufficientBalance: 'Not enough balance',
    InsufficientAllowance: 'The vault is not approved to spend this amount',
    Unauthorized: 'Only the admin may do that',
    UnbondNotFound: 'That unbond entry no longer exists',
    UnbondNotReady: 'The unbonding period has not ended',
    TokenRejected: 'The token rejected the transfer',
    TokenCall: 'The token program did not answer',
    NotEnoughGas: 'The transaction did not carry enough gas',
    FaucetCooldown: 'The faucet is cooling down for this address',
    FaucetDisabled: 'The faucet is switched off',
  };
  // Variant names arrive PascalCase from the IDL types but camelCase from decoded replies.
  const key = Object.keys(text).find((k) => k.toLowerCase() === variant.toLowerCase());
  const base = key ? text[key] : variant;
  return detail ? `${base} (${detail})` : base;
}

/**
 * Program results arrive in two shapes: `throws` functions come back unwrapped (sails-js throws
 * on an error reply), `Result` returns come back as `{ ok }` or `{ err }` objects.
 */
function unwrap<T, E>(res: Outcome<T, E> | T): T {
  if (res && typeof res === 'object' && 'err' in res) throw new ChainError(describeProgramError((res as { err: E }).err), 'PROGRAM');
  if (res && typeof res === 'object' && 'ok' in res) return (res as { ok: T }).ok;
  return res as T;
}

function num(v: unknown): number {
  return typeof v === 'bigint' ? Number(v) : typeof v === 'string' ? Number(v) : (v as number);
}
function big(v: unknown): bigint {
  return typeof v === 'bigint' ? v : BigInt(String(v));
}

function userMessage(e: unknown): ChainError {
  if (e instanceof ChainError) return e;
  const msg = e instanceof Error ? e.message : String(e);
  if (/Cancelled|Rejected by user|User denied/i.test(msg)) return new ChainError('You rejected the transaction in your wallet.', 'REJECTED');
  if (/1010|Inability to pay|balance too low/i.test(msg)) return new ChainError('Not enough VARA to pay the transaction fee.', 'INSUFFICIENT');
  // A `throws` function that failed arrives as an undecodable panic payload; say so plainly.
  if (/Panic occurred|0x474d|unable to decode|createType/i.test(msg)) return new ChainError('The program rejected the call. Check the amount and try again.', 'PROGRAM');
  return new ChainError(msg, 'UNKNOWN');
}

/**
 * Talks to the Vale Protocol programs on Vara through the generated sails-js clients.
 * Native VARA staking is not live yet: those calls fail with NOT_DEPLOYED.
 */
export class GearAdapter implements StakingAdapter {
  readonly kind = 'gear' as const;
  readonly simulated = false;
  readonly stakingLive = false;
  readonly programs: Partial<Record<VaultAsset, ProgramSet>>;
  private api: Promise<GearApiT> | null = null;
  private listeners = new Set<() => void>();
  private timer: ReturnType<typeof setInterval> | null = null;

  constructor(
    readonly network: NetworkId = 'mainnet',
    private opts: { programs?: Partial<Record<VaultAsset, ProgramSet>>; rpc?: string; signer?: SignerProvider; devFunder?: string } = {},
  ) {
    this.programs = opts.programs ?? readPrograms();
    const rpc = opts.rpc ?? NETWORKS[network].rpc;
    const funder = opts.devFunder ?? viteEnv.VITE_DEV_FUNDER_SURI;
    // Only ever on a local chain: a dev seed in the browser is fine there and nowhere else.
    if (funder && /127\.0\.0\.1|localhost/.test(rpc)) this.devFund = (address) => this.fundFromDev(address, funder);
  }

  devFund?: (address: string) => Promise<TxResult>;

  private async fundFromDev(address: string, suri: string): Promise<TxResult> {
    const api = await this.connect();
    const { GearKeyring } = await import('@gear-js/api');
    const pair = await GearKeyring.fromSuri(suri);
    return new Promise((resolve, reject) => {
      api.balance.transfer(address, 100_000_000_000_000).signAndSend(pair, ({ status, dispatchError, txHash }) => {
        if (dispatchError) reject(new ChainError(String(dispatchError), 'UNKNOWN'));
        else if (status.isInBlock) resolve({ hash: txHash.toHex() });
      }).catch((e: unknown) => reject(userMessage(e)));
    });
  }

  get deployed() { return VAULT_ASSETS.every((a) => !!this.programs[a]); }

  private async connect(): Promise<GearApiT> {
    if (!this.api) {
      this.api = import('@gear-js/api')
        .then(({ GearApi }) => GearApi.create({ providerAddress: this.opts.rpc ?? NETWORKS[this.network].rpc }))
        .catch((e: unknown) => { this.api = null; throw new ChainError(`Could not reach ${NETWORKS[this.network].label}: ${String(e)}`, 'RPC'); });
    }
    return this.api;
  }

  /** Generated clients for one asset, or null when its programs are not configured. */
  private async programsFor(asset: VaultAsset): Promise<{ token: DemoToken; vault: Vault }> {
    const ids = this.programs[asset];
    if (!ids) throw new ChainError(`The ${asset} vault is not deployed on ${NETWORKS[this.network].label} yet`, 'NOT_DEPLOYED');
    const api = await this.connect();
    const [{ DemoToken }, { Vault }] = await Promise.all([import('./idl/demo_token'), import('./idl/vault')]);
    return { token: new DemoToken(api, ids.token), vault: new Vault(api, ids.vault) };
  }

  private async hex(address: string): Promise<HexString> {
    const { decodeAddress } = await import('@gear-js/api');
    return decodeAddress(address);
  }

  private async send<T>(builder: TxBuilder<T>, address: string, gas: bigint): Promise<{ result: T } & TxResult> {
    try {
      const signer = await (this.opts.signer ?? extensionSigner)(address);
      builder.withAccount(signer.account, signer.options);
      builder.withGas(gas);
      const { txHash, blockHash, response } = await builder.signAndSend();
      const result = await response();
      const api = await this.connect();
      const header = await api.rpc.chain.getHeader(blockHash);
      return { result, hash: txHash, blockNumber: header.number.toNumber() };
    } catch (e) {
      throw userMessage(e);
    }
  }

  // --- reads --------------------------------------------------------------

  private async vaultStats(asset: VaultAsset): Promise<VaultStats | null> {
    if (!this.programs[asset]) return null;
    const { vault } = await this.programsFor(asset);
    const info = await vault.vault.info().call();
    const totalAssets = big(info.total_assets);
    return {
      rate: big(info.rate) / RATE_1E18_TO_1E9,
      apyBps: BigInt(num(info.apy_bps)),
      instantFeeBps: BigInt(num(info.instant_fee_bps)),
      unbondSecs: num(info.unbond_period_secs),
      minDeposit: big(info.min_deposit),
      totalShares: big(info.total_shares),
      totalAssets,
      holdings: big(info.holdings),
      tvlUsd: (Number(totalAssets) / Number(ONE_STABLE)) * STABLE_PRICE_USD,
      paused: !!info.paused,
    };
  }

  async getStats(): Promise<ProtocolStats> {
    const api = await this.connect();
    const [header, ...stats] = await Promise.all([api.rpc.chain.getHeader(), ...VAULT_ASSETS.map((a) => this.vaultStats(a))]);
    const empty: VaultStats = { rate: RATE_SCALE, apyBps: 0n, instantFeeBps: 0n, unbondSecs: 0, minDeposit: 0n, totalShares: 0n, totalAssets: 0n, holdings: 0n, tvlUsd: 0, paused: true };
    const vaults = Object.fromEntries(VAULT_ASSETS.map((a, i) => [a, stats[i] ?? empty])) as Record<VaultAsset, VaultStats>;
    const blockNumber = header.number.toNumber();
    const now = Date.now();
    return {
      rate: RATE_SCALE,
      stakeApyBps: 0n,
      vaults,
      tvlUsd: VAULT_ASSETS.reduce((s, a) => s + vaults[a].tvlUsd, 0),
      totalStakedVara: 0n,
      bufferBps: 0n,
      era: blockNumber,
      eraEndsAt: now,
      rateHistory: [],
      varaPriceUsd: 0,
      at: now,
      blockNumber,
    };
  }

  async getBalances(address: string): Promise<Balances> {
    const api = await this.connect();
    const who = await this.hex(address);
    const vara = await api.balance.findOut(address);
    const out: Balances = { VARA: vara.toBigInt(), kVARA: 0n, USDT: 0n, USDC: 0n, kUSDT: 0n, kUSDC: 0n };
    await Promise.all(VAULT_ASSETS.map(async (asset) => {
      if (!this.programs[asset]) return;
      const { token, vault } = await this.programsFor(asset);
      const [t, k] = await Promise.all([token.vft.balanceOf(who).call(), vault.vft.balanceOf(who).call()]);
      out[asset] = big(t);
      out[`k${asset}`] = big(k);
    }));
    return out;
  }

  async getUnbonding(address: string): Promise<UnbondEntry[]> {
    const who = await this.hex(address);
    const lists = await Promise.all(VAULT_ASSETS.map(async (asset): Promise<UnbondEntry[]> => {
      if (!this.programs[asset]) return [];
      const { vault } = await this.programsFor(asset);
      const entries = await vault.vault.unbonds(who).call();
      return entries.map((u) => ({ id: String(u.id), asset, amount: big(u.assets), startedAt: num(u.requested_at), claimableAt: num(u.claimable_at) }));
    }));
    return lists.flat().sort((a, b) => a.claimableAt - b.claimableAt);
  }

  async getFaucet(address: string): Promise<Record<VaultAsset, FaucetInfo>> {
    const who = await this.hex(address);
    const out = {} as Record<VaultAsset, FaucetInfo>;
    await Promise.all(VAULT_ASSETS.map(async (asset) => {
      if (!this.programs[asset]) { out[asset] = { amount: 0n, cooldownSecs: 0, nextClaimAt: 0 }; return; }
      const { token } = await this.programsFor(asset);
      const [cfg, next] = await Promise.all([token.faucet.config().call(), token.faucet.nextClaimAt(who).call()]);
      out[asset] = { amount: big(cfg.amount), cooldownSecs: num(cfg.cooldown_secs), nextClaimAt: num(next) };
    }));
    return out;
  }

  // --- native staking (not live) ---------------------------------------------

  private notLive(): never {
    throw new ChainError('Native VARA staking is not live on mainnet yet. Use the USDC and USDT vaults.', 'NOT_DEPLOYED');
  }
  stake(): Promise<TxResult> { return Promise.reject(this.safeNotLive()); }
  unstakeInstant(): Promise<TxResult> { return Promise.reject(this.safeNotLive()); }
  unstakeNative(): Promise<TxResult> { return Promise.reject(this.safeNotLive()); }
  private safeNotLive(): ChainError { try { this.notLive(); } catch (e) { return e as ChainError; } return new ChainError('unreachable'); }

  // --- vaults -----------------------------------------------------------------

  async claimFaucet(address: string, asset: VaultAsset): Promise<TxResult> {
    const { token } = await this.programsFor(asset);
    const r = await this.send(token.faucet.claim(), address, GAS.token);
    unwrap(r.result);
    return r;
  }

  async depositVault(address: string, asset: VaultAsset, amount: bigint): Promise<TxResult & { shares?: bigint }> {
    const ids = this.programs[asset];
    const { token, vault } = await this.programsFor(asset);
    const who = await this.hex(address);
    const allowance = await token.vft.allowance(who, ids!.vault).call();
    if (big(allowance) < amount) {
      const a = await this.send(token.vft.approve(ids!.vault, 2n ** 256n - 1n), address, GAS.token);
      unwrap(a.result);
    }
    const r = await this.send(vault.vault.deposit(amount), address, GAS.vaultAsync);
    return { hash: r.hash, blockNumber: r.blockNumber, shares: big(unwrap(r.result)) };
  }

  async redeemVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult & { net?: bigint; fee?: bigint }> {
    const { vault } = await this.programsFor(asset);
    const r = await this.send(vault.vault.redeem(shares), address, GAS.vaultAsync);
    const out = unwrap(r.result);
    return { hash: r.hash, blockNumber: r.blockNumber, net: big(out.net), fee: big(out.fee) };
  }

  async unbondVault(address: string, asset: VaultAsset, shares: bigint): Promise<TxResult & { amount?: bigint; claimableAt?: number }> {
    const { vault } = await this.programsFor(asset);
    const r = await this.send(vault.vault.requestUnbond(shares), address, GAS.vaultSync);
    const out = unwrap(r.result);
    return { hash: r.hash, blockNumber: r.blockNumber, amount: big(out.assets), claimableAt: num(out.claimable_at) };
  }

  async claimUnbond(address: string, asset: DepositAsset, id: string): Promise<TxResult> {
    if (asset === 'VARA') this.notLive();
    const { vault } = await this.programsFor(asset);
    const r = await this.send(vault.vault.claim(Number(id)), address, GAS.vaultAsync);
    unwrap(r.result);
    return r;
  }

  // --- lifecycle ----------------------------------------------------------------

  subscribe(cb: () => void) {
    this.listeners.add(cb);
    if (!this.timer) this.timer = setInterval(() => this.listeners.forEach((l) => l()), POLL_MS);
    return () => {
      this.listeners.delete(cb);
      if (this.listeners.size === 0 && this.timer) { clearInterval(this.timer); this.timer = null; }
    };
  }

  async dispose() {
    this.listeners.clear();
    if (this.timer) { clearInterval(this.timer); this.timer = null; }
    const api = this.api;
    this.api = null;
    if (api) { try { await (await api).disconnect(); } catch { /* already down */ } }
  }
}
