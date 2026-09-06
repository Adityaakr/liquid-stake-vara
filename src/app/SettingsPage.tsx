import { useOutletContext } from 'react-router';
import { Droplets, ExternalLink } from 'lucide-react';
import { Badge, Bento, Button } from '@/ui';
import { useStore } from '@/chain/store';
import { NETWORKS } from '@/chain/networks';
import { GearAdapter } from '@/chain/gearAdapter';
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
  const { adapter, network, account, balances, faucet, stats, run, tx, disconnect } = useStore();
  const now = useNow(1000);
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

      <Section title="Network">
        <Row k="Chain" v={net.label} />
        <Row k="RPC" v={net.rpc} />
        <Row k="Mode" v={adapter.simulated ? 'simulation' : adapter.deployed ? 'live programs' : 'programs not configured'} />
        {stats?.blockNumber !== undefined && <Row k="Latest block" v={stats.blockNumber.toLocaleString('en-US')} />}
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
