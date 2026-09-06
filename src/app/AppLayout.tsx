import { useEffect, useState } from 'react';
import { NavLink, Outlet, useLocation } from 'react-router';
import { ArrowLeft, BookOpen, Droplets, Settings, ShieldCheck, Vault, Wallet } from 'lucide-react';
import './app.css';
import { Badge, Bento, Button, Toast } from '@/ui';
import { useStore } from '@/chain/store';
import { NETWORKS } from '@/chain/networks';
import { formatCountdown, formatRate, shortAddress } from '@/domain/format';
import { useNow } from './bits';
import { WalletDialog } from './WalletDialog';
import { Wordmark } from '@/landing/Nav';

const TITLES: Record<string, string> = { '/app': 'Stake', '/app/vaults': 'Pools', '/app/portfolio': 'Portfolio', '/app/settings': 'Settings' };

function GlowBar({ pct, height = 6 }: { pct: number; height?: number }) {
  return (
    <div style={{ position: 'relative', height, background: 'var(--fx-mist)', borderRadius: 99, overflow: 'hidden' }}>
      <div style={{ width: `${pct}%`, height: '100%', borderRadius: 99, background: 'var(--grad-primary)' }} />
    </div>
  );
}

function useEraProgress() {
  const { stats } = useStore();
  const now = useNow();
  if (!stats) return { pct: 0, left: '—' };
  const eraMs = 12 * 60 * 60 * 1000;
  const left = stats.eraEndsAt - now;
  return { pct: Math.max(0, Math.min(100, ((eraMs - left) / eraMs) * 100)), left: formatCountdown(left) };
}

function NetworkCard() {
  const { stats, statsError, network, adapter, account, connectDemo, disconnect } = useStore();
  const { pct, left } = useEraProgress();
  const onChain = adapter.kind === 'gear';
  return (
    <Bento variant="app" pad={14}>
      {adapter.simulated && !account && (
        <button type="button" className="ap-demo" onClick={connectDemo}>Use a demo account</button>
      )}
      {account?.source === 'demo' && (
        <button type="button" className="ap-demo" onClick={disconnect}>Leave the demo account</button>
      )}
      <div style={{ display: 'flex', alignItems: 'center', gap: 8, fontSize: 12.5, color: 'var(--text-1)', fontWeight: 500 }}>
        {NETWORKS[network].label}
        {adapter.simulated && <Badge size="sm" tone="warn" style={{ marginLeft: 'auto' }}>simulation</Badge>}
        {onChain && !adapter.deployed && <Badge size="sm" tone="warn" style={{ marginLeft: 'auto' }}>not configured</Badge>}
      </div>
      {onChain ? (
        <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: 'var(--font-mono)', fontSize: 10.5, color: 'var(--text-3)', marginTop: 10 }}>
          <span>block {stats?.blockNumber ? stats.blockNumber.toLocaleString('en-US') : '—'}</span><span>{stats ? 'live' : 'connecting…'}</span>
        </div>
      ) : (
        <>
          <div style={{ margin: '10px 0 6px' }}><GlowBar pct={pct} height={5} /></div>
          <div style={{ display: 'flex', justifyContent: 'space-between', fontFamily: 'var(--font-mono)', fontSize: 10.5, color: 'var(--text-3)' }}>
            <span>era {stats ? stats.era.toLocaleString('en-US') : '—'}</span><span>ends {left}</span>
          </div>
        </>
      )}
      {statsError && <div title={statsError} style={{ marginTop: 8, fontSize: 11, color: 'var(--danger)' }}>node unreachable</div>}
    </Bento>
  );
}

const NAV = [
  { to: '/app', label: 'Stake', icon: Droplets, end: true },
  { to: '/app/vaults', label: 'Vaults', icon: Vault },
  { to: '/app/portfolio', label: 'Portfolio', icon: Wallet },
  { to: '/app/settings', label: 'Settings', icon: Settings },
];

function Sidebar() {
  return (
    <aside className="ap-side">
      <div style={{ display: 'flex', alignItems: 'center', gap: 10, padding: '0 12px', marginBottom: 28 }}>
        <Wordmark size={22} />
        <Badge size="sm" mono>app</Badge>
      </div>
      <nav aria-label="App" style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        {NAV.map(({ to, label, icon: Icon, end }) => (
          <NavLink key={to} to={to} end={end} className="ap-nav"><Icon size={17} strokeWidth={1.5} />{label}</NavLink>
        ))}
      </nav>
      <div className="eyebrow" style={{ fontSize: 12.5, margin: '24px 13px 8px' }}>Resources</div>
      <div style={{ display: 'flex', flexDirection: 'column', gap: 4 }}>
        <a className="ap-nav" href="https://wiki.vara.network/" target="_blank" rel="noreferrer"><BookOpen size={17} strokeWidth={1.5} />Docs</a>
        <a className="ap-nav" href="/#security"><ShieldCheck size={17} strokeWidth={1.5} />Audits</a>
        <NavLink className="ap-nav" to="/"><ArrowLeft size={17} strokeWidth={1.5} />Back to site</NavLink>
      </div>
      <div style={{ marginTop: 'auto' }}><NetworkCard /></div>
    </aside>
  );
}

function TopBar({ onWallet }: { onWallet: () => void }) {
  const { pathname } = useLocation();
  const { stats, account, adapter, connectDemo } = useStore();
  const title = TITLES[pathname] ?? 'Vale Protocol';
  return (
    <div className="ap-top">
      <div>
        <div style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 19, letterSpacing: 'var(--ls-heading)', lineHeight: 1.2 }}>{title}</div>
      </div>
      {adapter.stakingLive ? (
        <span className="ap-pill ap-rate" style={{ marginLeft: 'auto' }}>
          1 kVARA = <span style={{ color: 'var(--fx-indigo)', fontWeight: 600 }} className={stats ? undefined : 'skeleton'}>{stats ? formatRate(stats.rate) : '0.0000'}</span> VARA
        </span>
      ) : (
        <span className="ap-pill ap-rate" style={{ marginLeft: 'auto' }}>
          1 kUSDC = <span style={{ color: 'var(--fx-indigo)', fontWeight: 600 }} className={stats ? undefined : 'skeleton'}>{stats ? formatRate(stats.vaults.USDC.rate, 6) : '0.000000'}</span> USDC
        </span>
      )}
      {!account && adapter.simulated && <button type="button" className="ap-demo ap-demo-top" onClick={connectDemo}>Demo</button>}
      {account ? (
        <button type="button" onClick={onWallet} style={{ background: 'none', border: 'none', padding: 0, cursor: 'pointer', marginLeft: stats ? 0 : 'auto' }} aria-label="Account">
          <Badge tone="accent" mono dot>{shortAddress(account.address)}</Badge>
        </button>
      ) : (
        <Button size="md" onClick={onWallet} style={{ marginLeft: stats ? 0 : 'auto' }}>Connect wallet</Button>
      )}
    </div>
  );
}

function MobileBar() {
  return (
    <nav className="ap-mobile-bar" aria-label="App">
      {NAV.map(({ to, label, icon: Icon, end }) => (
        <NavLink key={to} to={to} end={end} className="ap-nav" style={{ flex: 1 }}><Icon size={18} strokeWidth={1.5} />{label}</NavLink>
      ))}
    </nav>
  );
}

export function AppLayout() {
  const [walletOpen, setWalletOpen] = useState(false);
  const { toasts, dismiss } = useStore();
  const { pathname } = useLocation();
  useEffect(() => { document.title = `${TITLES[pathname] ?? 'Vale Protocol'} · Vale Protocol app`; }, [pathname]);
  return (
    <div className="ap-root">
      <Sidebar />
      <main className="ap-main">
        <TopBar onWallet={() => setWalletOpen(true)} />
        <div className="ap-body">
          <Outlet context={{ openWallet: () => setWalletOpen(true) }} />
        </div>
      </main>
      <MobileBar />
      <WalletDialog open={walletOpen} onClose={() => setWalletOpen(false)} />
      <div style={{ position: 'fixed', right: 24, bottom: 24, zIndex: 120, display: 'flex', flexDirection: 'column', gap: 10 }}>
        {toasts.map((t) => <Toast key={t.id} tone={t.tone} title={t.title} detail={t.detail} onDismiss={() => dismiss(t.id)} />)}
      </div>
    </div>
  );
}

export type AppOutlet = { openWallet: () => void };
