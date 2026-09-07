import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState, type ReactNode } from 'react';
import type { VaultAsset } from '@/domain/protocol';
import { MockAdapter } from './mockAdapter';
import { GearAdapter } from './gearAdapter';
import { DEFAULT_NETWORK, NETWORKS } from './networks';
import { ChainError, type Account, type Balances, type FaucetInfo, type NetworkId, type ProtocolStats, type SessionInfo, type StakingAdapter, type TxResult, type TxStage, type UnbondEntry } from './types';
import { DEMO_ACCOUNT, connectWallet, recallAccount, rememberAccount } from './wallet';

export type ToastMsg = { id: number; tone: 'ok' | 'warn' | 'danger' | 'info'; title: string; detail?: string; link?: string };

type Store = {
  adapter: StakingAdapter;
  network: NetworkId;
  setNetwork: (n: NetworkId) => void;
  stats: ProtocolStats | null;
  statsError: string | null;
  account: Account | null;
  accounts: Account[];
  selectAccount: (a: Account) => void;
  connecting: boolean;
  connect: () => Promise<void>;
  connectDemo: () => void;
  disconnect: () => void;
  /** Adopt an account chosen through the official wallet provider (or clear it). */
  setExternalAccount: (a: Account | null) => void;
  balances: Balances | null;
  balancesError: string | null;
  unbonding: UnbondEntry[];
  faucet: Record<VaultAsset, FaucetInfo> | null;
  session: SessionInfo | null;
  refreshSession: () => Promise<void>;
  refresh: () => Promise<void>;
  tx: { stage: TxStage; label?: string; error?: string; result?: TxResult };
  run: (label: string, fn: (address: string) => Promise<TxResult>, onDone?: { title: string; detail?: string } | ((r: TxResult) => { title: string; detail?: string })) => Promise<boolean>;
  toasts: ToastMsg[];
  notify: (t: Omit<ToastMsg, 'id'>) => void;
  dismiss: (id: number) => void;
};

const Ctx = createContext<Store | null>(null);

function makeAdapter(network: NetworkId): StakingAdapter {
  const mode = import.meta.env.VITE_ADAPTER ?? 'mock';
  return mode === 'gear' ? new GearAdapter(network) : new MockAdapter(network);
}

export function errorMessage(e: unknown): string {
  if (e instanceof ChainError) return e.message;
  if (e instanceof Error) return e.message;
  return String(e);
}

export function StoreProvider({ children, adapter: injected }: { children: ReactNode; adapter?: StakingAdapter }) {
  const [network, setNetworkState] = useState<NetworkId>(DEFAULT_NETWORK);
  const adapter = useMemo(() => injected ?? makeAdapter(network), [injected, network]);
  const [stats, setStats] = useState<ProtocolStats | null>(null);
  const [statsError, setStatsError] = useState<string | null>(null);
  const [account, setAccount] = useState<Account | null>(() => {
    const a = recallAccount();
    if (a?.source === 'demo' && !adapter.simulated) { rememberAccount(null); return null; }
    return a;
  });
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [connecting, setConnecting] = useState(false);
  const [balances, setBalances] = useState<Balances | null>(null);
  const [balancesError, setBalancesError] = useState<string | null>(null);
  const [unbonding, setUnbonding] = useState<UnbondEntry[]>([]);
  const [faucet, setFaucet] = useState<Record<VaultAsset, FaucetInfo> | null>(null);
  const [session, setSession] = useState<SessionInfo | null>(null);
  const [tx, setTx] = useState<Store['tx']>({ stage: 'idle' });
  const [toasts, setToasts] = useState<ToastMsg[]>([]);
  const seq = useRef(0);
  const refreshSeq = useRef(0);
  const idleTimer = useRef<ReturnType<typeof setTimeout> | null>(null);
  const inFlight = useRef(false);

  const notify = useCallback((t: Omit<ToastMsg, 'id'>) => {
    const id = ++seq.current;
    setToasts((ts) => [...ts.slice(-2), { ...t, id }]);
    setTimeout(() => setToasts((ts) => ts.filter((x) => x.id !== id)), 6500);
  }, []);
  const dismiss = useCallback((id: number) => setToasts((ts) => ts.filter((x) => x.id !== id)), []);

  const loadStats = useCallback(async () => {
    try { setStats(await adapter.getStats()); setStatsError(null); } catch (e) { setStatsError(errorMessage(e)); }
  }, [adapter]);

  const refreshSession = useCallback(async () => {
    if (!account || !adapter.getSession) { setSession(null); return; }
    try { setSession(await adapter.getSession(account.address)); } catch { setSession(null); }
  }, [adapter, account]);

  const refresh = useCallback(async () => {
    const mine = ++refreshSeq.current;
    await loadStats();
    if (!account) { setBalances(null); setUnbonding([]); setFaucet(null); setSession(null); return; }
    void refreshSession();
    try {
      const [b, u, f] = await Promise.all([adapter.getBalances(account.address), adapter.getUnbonding(account.address), adapter.getFaucet(account.address)]);
      if (mine !== refreshSeq.current) return; // a newer refresh (other account or network) owns the state now
      setBalances(b); setUnbonding(u); setFaucet(f); setBalancesError(null);
    } catch (e) { if (mine === refreshSeq.current) setBalancesError(errorMessage(e)); }
  }, [adapter, account, loadStats, refreshSession]);

  useEffect(() => {
    let live = true;
    void Promise.resolve().then(() => { if (live) void refresh(); });
    return () => { live = false; };
  }, [refresh]);
  useEffect(() => adapter.subscribe(() => { void refresh(); }), [adapter, refresh]);
  useEffect(() => () => { void adapter.dispose(); }, [adapter]);
  useEffect(() => {
    const t = setInterval(() => { void loadStats(); }, 60_000);
    return () => clearInterval(t);
  }, [loadStats]);

  const connect = useCallback(async () => {
    setConnecting(true);
    try {
      const list = await connectWallet();
      setAccounts(list);
      const a = list[0];
      setAccount(a); rememberAccount(a);
      notify({ tone: 'ok', title: 'Wallet connected', detail: `${a.name ?? 'Account'} · ${NETWORKS[adapter.network].label}` });
    } catch (e) {
      notify({ tone: 'danger', title: 'Could not connect', detail: errorMessage(e) });
      throw e;
    } finally { setConnecting(false); }
  }, [adapter.network, notify]);

  const connectDemo = useCallback(() => {
    setAccount(DEMO_ACCOUNT); setAccounts([DEMO_ACCOUNT]); rememberAccount(DEMO_ACCOUNT);
    notify({ tone: 'info', title: 'Demo account connected', detail: 'Balances are simulated. Nothing is sent on chain.' });
  }, [notify]);

  const disconnect = useCallback(() => { setAccount(null); setAccounts([]); rememberAccount(null); setBalances(null); setUnbonding([]); setFaucet(null); }, []);
  const selectAccount = useCallback((a: Account) => { setAccount(a); rememberAccount(a); }, []);
  const setExternalAccount = useCallback((a: Account | null) => {
    setAccount((prev) => {
      if (a === null) return prev?.source === 'demo' ? prev : null;
      return prev?.address === a.address && prev.source === a.source ? prev : a;
    });
    setAccounts(a ? [a] : []);
    if (a) rememberAccount(a); else rememberAccount(null);
  }, []);
  const setNetwork = useCallback((n: NetworkId) => { setNetworkState(n); setStats(null); setStatsError(null); setBalances(null); setBalancesError(null); setUnbonding([]); setFaucet(null); }, []);

  const run = useCallback<Store['run']>(async (label, fn, onDone) => {
    if (!account || inFlight.current) return false;
    inFlight.current = true;
    if (idleTimer.current) { clearTimeout(idleTimer.current); idleTimer.current = null; }
    const settle = (delay: number) => { idleTimer.current = setTimeout(() => { idleTimer.current = null; setTx({ stage: 'idle' }); }, delay); };
    setTx({ stage: 'broadcast', label });
    try {
      const result = await fn(account.address);
      setTx({ stage: 'finalized', label, result });
      await refresh();
      if (onDone) notify({ tone: 'ok', ...(typeof onDone === 'function' ? onDone(result) : onDone), link: result.link });
      settle(1500);
      return true;
    } catch (e) {
      const error = errorMessage(e);
      setTx({ stage: 'error', label, error });
      notify({ tone: 'danger', title: `${label} failed`, detail: error });
      settle(2500);
      return false;
    } finally { inFlight.current = false; }
  }, [account, refresh, notify]);

  const value: Store = { adapter, network, setNetwork, stats, statsError, account, accounts, selectAccount, connecting, connect, connectDemo, disconnect, setExternalAccount, balances, balancesError, unbonding, faucet, session, refreshSession, refresh, tx, run, toasts, notify, dismiss };
  return <Ctx.Provider value={value}>{children}</Ctx.Provider>;
}

export function useStore(): Store {
  const s = useContext(Ctx);
  if (!s) throw new Error('useStore must be used inside StoreProvider');
  return s;
}

export type { VaultAsset };
