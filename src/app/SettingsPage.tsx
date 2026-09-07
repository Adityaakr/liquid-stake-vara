import { useOutletContext } from 'react-router';
import { useState } from 'react';
import { Droplets, ExternalLink, Zap } from 'lucide-react';
import { Badge, Bento, Button } from '@/ui';
import { useStore } from '@/chain/store';
import { NETWORKS } from '@/chain/networks';
import { GearAdapter, SESSION_DEFAULT_GAS_VARA, SESSION_MIN_GAS } from '@/chain/gearAdapter';
import { formatCountdown, formatStable, formatVara, shortAddress } from '@/domain/format';
import { VAULT_ASSETS } from '@/domain/protocol';
import { Row, useNow } from './bits';
import type { AppOutlet } from './AppLayout';

function Section({ title, children, aside }: { title: string; children: React.ReactNode; aside?: React.ReactNode }) {
  return (
    <Bento variant="app" pad={26}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, marginBottom: 6 }}>
        <div style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 18 }}>{title}</div>
        {aside}
      </div>
      {children}
    </Bento>
  );
}

export function SettingsPage() {
  const { openWallet } = useOutletContext<AppOutlet>();
  const { adapter, network, account, balances, faucet, stats, run, tx, disconnect, session, refreshSession } = useStore();
  const now = useNow(1000);
  const [hours, setHours] = useState(24);
  const canSession = !!adapter.enableSession && adapter.deployed;
  const sessionLive = !!session && session.expiresAt > now;
  const busy = tx.stage === 'broadcast';
  const net = NETWORKS[network];
  const programs = adapter instanceof GearAdapter ? adapter.programs : undefined;
  const lowVara = !!account && !!balances && balances.VARA < 2n * 10n ** 12n;

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20, maxWidth: 820 }}>
      <Section title="Demo tokens" aside={<Badge tone="info" size="sm">for testing</Badge>}>
        <p style={{ fontSize: 14, color: 'var(--text-2)', lineHeight: 1.65, margin: '0 0 16px' }}>
          USDT and USDC on Vale Protocol are demo tokens with no monetary value. Anyone with a little VARA for gas can claim
          them here once per cooldown, deposit them into the kUSDT and kUSDC pools, and try the full flow: deposit, watch the
          rate accrue, withdraw instantly or unbond and claim. Each claim is an on-chain transaction, so you sign it in your wallet
          and pay a small fee in VARA.
        </p>
        {!account ? (
          <Button onClick={openWallet}>Connect a wallet to claim</Button>
        ) : (
          <div style={{ display: 'flex', flexDirection: 'column', gap: 10 }}>
            {lowVara && (
              <p role="alert" style={{ fontSize: 13, color: '#7A4B00', background: '#FFF7E6', border: '1px solid #F5D9A8', borderRadius: 12, padding: '10px 12px', lineHeight: 1.5 }}>
                Your wallet holds {formatVara(balances!.VARA)} VARA. Claims and deposits need a fraction of a VARA each for fees, so top up first.
                {adapter.devFund && ' On this local chain you can take test VARA below.'}
              </p>
            )}
            {adapter.devFund && (
              <Button variant="secondary" loading={busy && tx.label === 'Dev funding'} onClick={() => run('Dev funding', (addr) => adapter.devFund!(addr), { title: 'Received 100 VARA', detail: 'Sent from the //Alice dev account on the local node.' })}>Get 100 VARA for fees (local chain)</Button>
            )}
            {VAULT_ASSETS.map((asset) => {
              const f = faucet?.[asset];
              const ready = !!f && f.amount > 0n && now >= f.nextClaimAt;
              const label = !f ? 'Loading…' : f.amount === 0n ? 'Faucet is off' : ready ? `Get ${formatStable(f.amount, 0)} demo ${asset}` : `Available again in ${formatCountdown(f.nextClaimAt - now)}`;
              return (
                <div key={asset} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 12, background: '#fff', border: '1px solid var(--fx-mist)', borderRadius: 16, padding: '12px 14px', flexWrap: 'wrap' }}>
                  <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
                    <span style={{ fontSize: 14, fontWeight: 600 }}>Demo {asset}</span>
                    <span style={{ fontSize: 12.5, color: 'var(--text-3)' }}>{f ? `${formatStable(f.amount, 0)} per claim · every ${Math.round(f.cooldownSecs / 3600) || 1} h` : ''} · wallet {balances ? formatStable(balances[asset]) : '…'} {asset}</span>
                  </div>
                  <Button size="md" disabled={!adapter.deployed || !ready} loading={busy && tx.label === `${asset} faucet`} onClick={() => run(`${asset} faucet`, (addr) => adapter.claimFaucet(addr, asset), { title: `Received ${formatStable(f?.amount ?? 0n)} demo ${asset}`, detail: 'Deposit them in the vaults to try the protocol.' })}>
                    <Droplets size={14} strokeWidth={1.5} style={{ marginRight: 6 }} />{label}
                  </Button>
                </div>
              );
            })}
          </div>
        )}
      </Section>

      {canSession && (
        <Section title="One-click transactions" aside={sessionLive ? <Badge tone="ok" size="sm">active</Badge> : <Badge tone="neutral" size="sm">off</Badge>}>
          <p style={{ fontSize: 14, color: 'var(--text-2)', lineHeight: 1.65, margin: '0 0 16px' }}>
            Skip the wallet prompt on every action. Enabling a session takes one signature: it approves the pools, registers a key that
            lives only in this browser, and moves a little VARA to that key for fees (Vara reserves 10 VARA per action while it runs and refunds
            almost all of it, so the key needs a small float). Deposits, withdrawals, unbonds and claims are then
            sent straight to chain, and every action shows a link to the transaction. Payouts always go to your own account; the key can
            do nothing else. Revoke at any time to return its leftover VARA.
          </p>
          {!account ? (
            <Button onClick={openWallet}>Connect a wallet first</Button>
          ) : sessionLive ? (
            <>
              <Row k="Session key" v={shortAddress(session!.key, 10, 6)} />
              <Row k="Expires" v={`in ${formatCountdown(session!.expiresAt - now)}`} />
              <Row k="VARA for fees" v={`${formatVara(session!.gasBalance)} VARA${session!.gasBalance < SESSION_MIN_GAS ? ' (low, your wallet signs until it is topped up)' : ''}`} />
              <Row k="Allowed" v={session!.actions.join(', ').toLowerCase()} />
              <div style={{ display: 'flex', gap: 10, marginTop: 14, flexWrap: 'wrap' }}>
                <Button variant="secondary" loading={busy && tx.label === 'Revoke session'} onClick={() => run('Revoke session', (addr) => adapter.revokeSession!(addr), { title: 'Session revoked', detail: 'Leftover VARA was returned to your account. Actions ask your wallet again.' }).then(() => refreshSession())}>Revoke session</Button>
              </div>
            </>
          ) : (
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, flexWrap: 'wrap' }}>
              <label style={{ fontSize: 13.5, color: 'var(--text-2)', display: 'inline-flex', alignItems: 'center', gap: 8 }}>
                Duration
                <select value={hours} onChange={(e) => setHours(Number(e.target.value))} style={{ height: 36, borderRadius: 10, border: '1px solid var(--fx-mist)', padding: '0 10px', fontFamily: 'var(--font-body)', background: '#fff' }}>
                  <option value={1}>1 hour</option><option value={24}>24 hours</option><option value={168}>7 days</option><option value={720}>30 days</option>
                </select>
              </label>
              <Button loading={busy && tx.label === 'Enable session'} onClick={() => run('Enable session', (addr) => adapter.enableSession!(addr, { hours, gasVara: SESSION_DEFAULT_GAS_VARA }), { title: 'One-click transactions on', detail: `Pools approved and a session key funded with ${SESSION_DEFAULT_GAS_VARA} VARA for ${hours >= 24 ? `${hours / 24} day${hours > 24 ? 's' : ''}` : `${hours} hour`}.` }).then(() => refreshSession())}>
                <Zap size={14} strokeWidth={1.5} style={{ marginRight: 6 }} />Enable with one signature
              </Button>
            </div>
          )}
        </Section>
      )}

      <Section title="Network">
        <Row k="Chain" v={net.label} />
        <Row k="RPC" v={net.rpc} />
        <Row k="Mode" v={adapter.simulated ? 'simulation' : adapter.deployed ? 'live programs' : 'programs not configured'} />
        {stats?.blockNumber !== undefined && <Row k="Latest block" v={stats.blockNumber.toLocaleString('en-US')} />}
        {adapter instanceof GearAdapter && (
          adapter.pool
            ? <Row k="kVARA pool" v={<a href={`${net.explorer}/account/${adapter.pool}`} target="_blank" rel="noreferrer" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>{shortAddress(adapter.pool, 10, 6)} <ExternalLink size={12} /></a>} />
            : <Row k="kVARA pool" v="not configured" />
        )}
        {programs && VAULT_ASSETS.map((asset) => {
          const ids = programs[asset];
          if (!ids) return <Row key={asset} k={`${asset} programs`} v="not configured" />;
          return (
            <div key={asset}>
              <Row k={`${asset} token`} v={<a href={`${net.explorer}/account/${ids.token}`} target="_blank" rel="noreferrer" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>{shortAddress(ids.token, 10, 6)} <ExternalLink size={12} /></a>} />
              <Row k={`k${asset} vault`} v={<a href={`${net.explorer}/account/${ids.vault}`} target="_blank" rel="noreferrer" style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>{shortAddress(ids.vault, 10, 6)} <ExternalLink size={12} /></a>} />
            </div>
          );
        })}
      </Section>

      <Section title="Account">
        {account ? (
          <>
            <Row k="Address" v={shortAddress(account.address, 10, 8)} />
            <Row k="Wallet" v={account.source} />
            <div style={{ display: 'flex', gap: 10, marginTop: 14 }}>
              <Button variant="secondary" onClick={openWallet}>Switch account</Button>
              <Button variant="ghost" onClick={disconnect}>Disconnect</Button>
            </div>
          </>
        ) : (
          <Button onClick={openWallet}>Connect wallet</Button>
        )}
      </Section>
    </div>
  );
}
