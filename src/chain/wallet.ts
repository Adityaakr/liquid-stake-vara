import { ChainError, type Account } from './types';

export const APP_NAME = 'Vale Protocol';
const KEY = 'vale.wallet.v1';

/** Known injected wallets and how they name themselves in window.injectedWeb3. */
export const WALLETS = [
  { id: 'polkadot-js', label: 'Polkadot.js', url: 'https://polkadot.js.org/extension/' },
  { id: 'subwallet-js', label: 'SubWallet', url: 'https://subwallet.app/' },
  { id: 'talisman', label: 'Talisman', url: 'https://talisman.xyz/' },
  { id: 'nova', label: 'Nova', url: 'https://novawallet.io/' },
] as const;

export function detectWallets(): string[] {
  const w = (globalThis as { injectedWeb3?: Record<string, unknown> }).injectedWeb3 ?? {};
  return Object.keys(w);
}

export async function connectWallet(ss58 = 137): Promise<Account[]> {
  const { web3Enable, web3Accounts } = await import('@polkadot/extension-dapp');
  const injected = await web3Enable(APP_NAME);
  if (injected.length === 0) {
    if (detectWallets().length > 0) throw new ChainError('The wallet denied access to Vale Protocol. Open the extension and allow this site.', 'REJECTED');
    throw new ChainError('No Substrate wallet extension found. Install Polkadot.js, SubWallet or Talisman.', 'NO_WALLET');
  }
  const accounts = await web3Accounts({ ss58Format: ss58 });
  if (accounts.length === 0) throw new ChainError('The wallet has no accounts, or access was denied.', 'REJECTED');
  return accounts.map((a) => ({ address: a.address, name: a.meta.name, source: a.meta.source }));
}

export function rememberAccount(a: Account | null) {
  try {
    if (a) localStorage.setItem(KEY, JSON.stringify(a));
    else localStorage.removeItem(KEY);
  } catch { /* ignore */ }
}
export function recallAccount(): Account | null {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return null;
    const a = JSON.parse(raw) as Partial<Account> | null;
    if (a && typeof a.address === 'string' && a.address.length > 0 && typeof a.source === 'string') return { address: a.address, name: typeof a.name === 'string' ? a.name : undefined, source: a.source };
    localStorage.removeItem(KEY);
    return null;
  } catch { return null; }
}

/** A stable demo account used when the app runs in simulation without a wallet. Derived from a fixed seed; nobody holds its key. */
export const DEMO_ACCOUNT: Account = { address: 'kGj1akEAemmGoVyFqeHVSNUyUj1mJp3gazUYy7Zs2p7BsgT88', name: 'Demo', source: 'demo' };
