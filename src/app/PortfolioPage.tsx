import { useNavigate, useOutletContext } from 'react-router';
import { Badge, Bento, Button, Stat, TokenBadge, type TokenSymbol } from '@/ui';
import { useStore } from '@/chain/store';
import { sharesToAssets, kVaraToVara } from '@/domain/math';
import { bpsToPercent, formatCountdown, formatRate, formatStable, formatUsd, formatVara, toNumber } from '@/domain/format';
import { STABLE_DECIMALS, VARA_DECIMALS, VAULT_ASSETS } from '@/domain/protocol';
import type { AppOutlet } from './AppLayout';
import { useNow } from './bits';

export function PortfolioPage() {
  const nav = useNavigate();
  const { openWallet } = useOutletContext<AppOutlet>();
  const { stats, balances, unbonding, account, adapter, run, tx } = useStore();
  const now = useNow();

  const price = stats?.varaPriceUsd ?? 0;
  const stakedV = stats && balances ? kVaraToVara(balances.kVARA, stats.rate) : 0n;
  const stableUsd = stats && balances ? VAULT_ASSETS.reduce((s, a) => s + toNumber(sharesToAssets(balances[`k${a}`], stats.vaults[a].rate), STABLE_DECIMALS), 0) : 0;
  const unbondingV = unbonding.filter((u) => u.asset === 'VARA').reduce((a, u) => a + u.amount, 0n);
  const unbondingStable = unbonding.filter((u) => u.asset !== 'VARA').reduce((a, u) => a + toNumber(u.amount, STABLE_DECIMALS), 0);
  const totalUsd = toNumber(stakedV + unbondingV, VARA_DECIMALS) * price + stableUsd + unbondingStable;
  const fmtAmount = (u: (typeof unbonding)[number]) => (u.asset === 'VARA' ? `${formatVara(u.amount)} VARA` : `${formatStable(u.amount)} ${u.asset}`);
  const unbondingLabel = unbonding.length === 0 ? '—' : unbondingStable === 0 ? `${formatVara(unbondingV)} VARA` : unbondingV === 0n ? formatUsd(unbondingStable) : `${formatVara(unbondingV)} VARA + ${formatUsd(unbondingStable)}`;

  type RowT = { tok: TokenSymbol; amt: string; rate: string; val: string; apy: string; status: ['accent' | 'ok', string] };
  const all: (RowT & { raw: bigint })[] = stats && balances ? [
    { tok: 'kVARA', amt: formatVara(balances.kVARA), rate: formatRate(stats.rate), val: formatUsd(toNumber(stakedV, VARA_DECIMALS) * price), apy: bpsToPercent(stats.stakeApyBps), status: ['accent', 'earning'], raw: balances.kVARA },
    ...VAULT_ASSETS.map((a): RowT & { raw: bigint } => ({ tok: `k${a}` as TokenSymbol, amt: formatStable(balances[`k${a}`]), rate: formatRate(stats.vaults[a].rate), val: formatUsd(toNumber(sharesToAssets(balances[`k${a}`], stats.vaults[a].rate), STABLE_DECIMALS)), apy: bpsToPercent(stats.vaults[a].apyBps), status: ['ok', 'accruing'], raw: balances[`k${a}`] })),
  ] : [];
  const rows: RowT[] = all.filter((r) => r.raw > 0n).map(({ raw: _r, ...rest }) => rest);

  const busy = tx.stage === 'broadcast';
  const loading = !!account && (!balances || !stats);

  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      <div className="ap-grid-4">
        <Bento variant="app" pad={20}><Stat label="Total value" value={formatUsd(totalUsd)} size="sm" gradient loading={loading} /></Bento>
        <Bento variant="app" pad={20}><Stat label="Staked" value={`${balances ? formatVara(balances.kVARA) : '0.00'} kVARA`} size="sm" mono sub={adapter.stakingLive ? `≈ ${formatVara(stakedV)} VARA` : 'coming soon on mainnet'} loading={loading} /></Bento>
        <Bento variant="app" pad={20}><Stat label="In vaults" value={formatUsd(stableUsd)} size="sm" mono loading={loading} /></Bento>
        <Bento variant="app" pad={20}><Stat label="Unbonding" value={unbondingLabel} size="sm" mono sub={unbonding.length ? `claimable in ${formatCountdown(Math.min(...unbonding.map((u) => u.claimableAt)) - now)}` : '—'} loading={loading} /></Bento>
      </div>

      <Bento variant="app" pad={0}>
        <div style={{ padding: '20px 26px 14px', display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
          <span style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 18 }}>Positions</span>
          <Badge size="sm" mono>{rows.length} active</Badge>
        </div>
        {!account ? (
          <div style={{ padding: '40px 24px 48px', textAlign: 'center' }}>
            <p style={{ color: 'var(--text-3)', fontSize: 14 }}>Connect a wallet to see your positions.</p>
            <div style={{ display: 'flex', gap: 10, justifyContent: 'center', marginTop: 18 }}><Button size="md" onClick={openWallet}>Connect wallet</Button></div>
          </div>
        ) : rows.length === 0 ? (
          <div style={{ padding: '40px 24px 48px', textAlign: 'center' }}>
            <p style={{ color: 'var(--text-3)', fontSize: 14 }}>{loading ? 'Loading positions…' : "No positions yet — the rate can't rise for a balance of zero."}</p>
            {!loading && (
              <div style={{ display: 'flex', gap: 10, justifyContent: 'center', marginTop: 18 }}>
                <Button size="md" onClick={() => nav('/app/vaults')}>Deposit in a pool</Button>
                {adapter.stakingLive && <Button size="md" variant="secondary" onClick={() => nav('/app')}>Stake VARA</Button>}
              </div>
            )}
          </div>
        ) : (
          <div className="ap-table-wrap">
            <table className="ap-table">
              <thead><tr>{['Asset', 'Balance', 'Rate / share', 'Value', 'APY', 'Status'].map((h) => <th key={h} className="ap-th">{h}</th>)}</tr></thead>
              <tbody>
                {rows.map((r) => (
                  <tr key={r.tok} className="ap-row">
                    <td style={{ fontFamily: 'var(--font-body)' }}><TokenBadge token={r.tok} size={30} /></td>
                    <td>{r.amt}</td>
                    <td style={{ color: 'var(--text-2)' }}>{r.rate}</td>
                    <td>{r.val}</td>
                    <td style={{ color: 'var(--fx-indigo)' }}>{r.apy}</td>
                    <td><Badge tone={r.status[0]} size="sm" dot>{r.status[1]}</Badge></td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
        {unbonding.map((u) => {
          const ready = now >= u.claimableAt;
          return (
            <div key={`${u.asset}-${u.id}`} style={{ padding: '14px 26px', borderTop: '1px solid var(--line-1)', display: 'flex', justifyContent: 'space-between', alignItems: 'center', gap: 12, flexWrap: 'wrap' }}>
              <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
                <Badge tone={ready ? 'ok' : 'info'} size="sm">{ready ? 'Claimable' : 'Unbonding'}</Badge>
                <span style={{ fontSize: 13, color: 'var(--text-2)' }}>{fmtAmount(u)} at full rate</span>
              </div>
              {ready ? (
                <Button size="sm" loading={busy} onClick={() => run('Claim', (addr) => adapter.claimUnbond(addr, u.asset, u.id), { title: `Claimed ${fmtAmount(u)}` })}>Claim</Button>
              ) : (
                <span style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text-3)' }}>claim in {formatCountdown(u.claimableAt - now)}</span>
              )}
            </div>
          );
        })}
      </Bento>
      <p style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--text-3)' }}>
        {adapter.simulated ? 'Balances are illustrative. ' : ''}Yield is embedded in rates — no claiming, ever.
      </p>
    </div>
  );
}
