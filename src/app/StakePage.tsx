import { useMemo, useState } from 'react';
import { useOutletContext } from 'react-router';
import { ChartPie, Coins, Star, Wallet } from 'lucide-react';
import { AmountField, Badge, Bento, Button, Tabs, type TokenSymbol } from '@/ui';
import { useStore } from '@/chain/store';
import type { DepositAsset } from '@/chain/types';
import { accrueRate, assetsToShares, sharesToAssets, validateAmount } from '@/domain/math';
import { bpsToPercent, formatCompactUsd, formatCountdown, formatRate, formatUnits, formatUsd, parseUnits, toNumber } from '@/domain/format';
import { BPS, INSTANT_UNSTAKE_FEE_BPS, STABLE_DECIMALS, VARA_DECIMALS } from '@/domain/protocol';
import { Eyebrow, IRow, useNow } from './bits';
import type { AppOutlet } from './AppLayout';

type Tab = 'Stake' | 'Unstake';
type Path = 'instant' | 'native';

/** Everything the stake box needs per asset. Receipts are always k-prefixed. */
const ASSETS: Record<DepositAsset, { deposit: TokenSymbol; receipt: TokenSymbol; decimals: number }> = {
  VARA: { deposit: 'VARA', receipt: 'kVARA', decimals: VARA_DECIMALS },
  USDT: { deposit: 'wUSDT', receipt: 'kUSDT', decimals: STABLE_DECIMALS },
  USDC: { deposit: 'wUSDC', receipt: 'kUSDC', decimals: STABLE_DECIMALS },
};
const ORDER: DepositAsset[] = ['VARA', 'USDT', 'USDC'];
const assetOf = (t: TokenSymbol): DepositAsset => (t.replace(/^[wk]/, '') as DepositAsset);

function ExitOption({ title, sub, active, onClick }: { title: string; sub: string; active: boolean; onClick: () => void }) {
  return (
    <button type="button" className="ap-exit" aria-pressed={active} onClick={onClick}>
      <div className="ap-exit-t">{title}</div>
      <div className="ap-exit-s">{sub}</div>
    </button>
  );
}

const REASON: Record<string, string> = { invalid: 'Enter a valid amount', zero: 'Amount must be more than zero', insufficient: 'Not enough balance' };

export function StakePage() {
  const { openWallet } = useOutletContext<AppOutlet>();
  const { stats, statsError, balances, balancesError, account, adapter, tx, run } = useStore();
  const [tab, setTab] = useState<Tab>('Stake');
  const [asset, setAsset] = useState<DepositAsset>('VARA');
  const [amt, setAmt] = useState('');
  const [path, setPath] = useState<Path>('instant');
  const [touched, setTouched] = useState(false);
  const now = useNow();
  const reset = () => { setAmt(''); setTouched(false); };
  const switchTab = (t: Tab) => { setTab(t); reset(); };
  const switchAsset = (t: TokenSymbol) => { setAsset(assetOf(t)); setPath('instant'); reset(); };

  const { deposit, receipt, decimals } = ASSETS[asset];
  const isVara = asset === 'VARA';
  const fmt = (v: bigint, dp = 2) => formatUnits(v, decimals, dp);
  const apyBps = stats ? (isVara ? stats.stakeApyBps : stats.vaultApyBps[asset]) : null;
  // Rates keep accruing between stat refreshes; project them to the current second.
  const rate = stats ? accrueRate(isVara ? stats.rate : stats.vaultSharePrice[asset], apyBps ?? 0n, now - stats.at) : null;
  const parsed = useMemo(() => parseUnits(amt, decimals), [amt, decimals]);
  const bal = tab === 'Stake' ? balances?.[deposit as 'VARA' | 'wUSDT' | 'wUSDC'] ?? 0n : balances?.[receipt as 'kVARA' | 'kUSDT' | 'kUSDC'] ?? 0n;
  const validation = validateAmount(amt, parsed, balances ? bal : null);
  const error = touched && !validation.ok && validation.reason !== 'empty' ? REASON[validation.reason] : undefined;

  const out = useMemo(() => {
    if (!rate || !parsed) return { main: 0n, fee: 0n };
    if (tab === 'Stake') return { main: assetsToShares(parsed, rate), fee: 0n };
    const gross = sharesToAssets(parsed, rate);
    if (path === 'instant' || !isVara) { const fee = (gross * INSTANT_UNSTAKE_FEE_BPS) / BPS; return { main: gross - fee, fee }; }
    return { main: gross, fee: 0n };
  }, [rate, parsed, tab, path, isVara]);

  const price = isVara ? stats?.varaPriceUsd ?? 0 : 1;
  const usd = (v: bigint) => formatUsd(toNumber(v, decimals) * price);
  const busy = tx.stage === 'broadcast';

  const act = async () => {
    if (!account) { openWallet(); return; }
    setTouched(true);
    if (!validation.ok || !parsed || !rate) return;
    const a = parsed;
    let ok = false;
    if (tab === 'Stake') {
      ok = isVara
        ? await run('Stake', (addr) => adapter.stake(addr, a), { title: `Staked ${fmt(a)} VARA`, detail: `You received ${fmt(out.main)} kVARA at rate ${formatRate(rate)}.` })
        : await run(`Deposit ${deposit}`, (addr) => adapter.depositVault(addr, asset, a), { title: `Deposited ${fmt(a)} ${deposit}`, detail: `You received ${fmt(out.main)} ${receipt} at share price ${formatRate(rate)}.` });
    } else if (!isVara) {
      ok = await run(`Withdraw ${asset}`, (addr) => adapter.redeemVault(addr, asset, a), { title: `Withdrew ${fmt(out.main)} ${deposit}`, detail: '0.3% instant exit fee applied.' });
    } else if (path === 'instant') {
      ok = await run('Instant unstake', (addr) => adapter.unstakeInstant(addr, a), { title: 'Unstaked instantly', detail: `${fmt(out.main)} VARA received · 0.3% fee applied.` });
    } else {
      ok = await run('Native unbond', (addr) => adapter.unstakeNative(addr, a), { title: 'Unbond started', detail: `${fmt(out.main)} VARA claimable in 7 days at full rate.` });
    }
    if (ok) reset();
  };

  const label = !account ? 'Connect wallet' : tab === 'Stake' ? (isVara ? 'Stake' : `Deposit ${deposit}`) : !isVara ? 'Withdraw instantly' : path === 'instant' ? 'Unstake instantly' : 'Start unbond';
  const pickable = tab === 'Stake' ? ORDER.map((a) => ASSETS[a].deposit) : ORDER.map((a) => ASSETS[a].receipt);

  return (
    <div className="ap-stake">
      <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 12, background: 'var(--fx-lgb)', border: '1px solid var(--fx-lgb)', borderRadius: 20, padding: '12px 18px', flexWrap: 'wrap' }}>
          <span style={{ width: 30, height: 30, borderRadius: 99, background: '#fff', display: 'inline-flex', alignItems: 'center', justifyContent: 'center', color: 'var(--text-2)' }}><Wallet size={14} strokeWidth={1.5} /></span>
          <span style={{ fontSize: 13.5, color: 'var(--text-2)' }}>Wallet balance</span>
          {account ? (
            balancesError ? <span style={{ marginLeft: 'auto', fontSize: 13, color: 'var(--danger)' }}>{balancesError}</span> : (
              <span className={balances ? undefined : 'skeleton'} style={{ marginLeft: 'auto', fontFamily: 'var(--font-mono)', fontSize: 14, minWidth: balances ? undefined : 140 }}>
                {balances ? <>{fmt(balances[deposit as 'VARA' | 'wUSDT' | 'wUSDC'])} {deposit} <span style={{ color: 'var(--text-3)' }}>( {usd(balances[deposit as 'VARA' | 'wUSDT' | 'wUSDC'])} )</span></> : '0.00'}
              </span>
            )
          ) : (
            <span style={{ marginLeft: 'auto', fontFamily: 'var(--font-mono)', fontSize: 14, color: 'var(--text-3)' }}>— {deposit}</span>
          )}
        </div>

        <Bento variant="app" pad={24}>
          <Tabs<Tab> items={['Stake', 'Unstake']} active={tab} onChange={switchTab} />
          <div style={{ marginTop: 16 }}>
            <AmountField
              label={tab === 'Stake' ? 'You stake' : 'You unstake'}
              token={tab === 'Stake' ? deposit : receipt}
              tokens={pickable}
              onToken={switchAsset}
              balance={balances ? fmt(bal) : undefined}
              value={amt}
              onChange={(v) => { setAmt(v); setTouched(true); }}
              onMax={balances ? () => { setAmt(fmt(bal, decimals).replace(/,/g, '').replace(/\.?0+$/, '')); setTouched(true); } : undefined}
              fiat={parsed && rate ? `≈ ${usd(tab === 'Stake' ? parsed : sharesToAssets(parsed, rate))}` : ''}
              error={error}
              disabled={busy}
            />
          </div>
          {tab === 'Unstake' && isVara && (
            <div style={{ display: 'flex', gap: 10, marginTop: 12 }}>
              <ExitOption title="Instant" sub="~0.3% fee · now" active={path === 'instant'} onClick={() => setPath('instant')} />
              <ExitOption title="Native unbond" sub="free · 7 days" active={path === 'native'} onClick={() => setPath('native')} />
            </div>
          )}
          {tab === 'Unstake' && !isVara && (
            <p style={{ fontSize: 12.5, color: 'var(--text-3)', margin: '10px 2px 0', lineHeight: 1.5 }}>Stable vaults exit instantly with a 0.3% fee; there is no unbonding period.</p>
          )}
          <div style={{ margin: '10px 0 14px' }}>
            <IRow icon={<ChartPie size={14} strokeWidth={1.5} />} k="Position" v={balances ? `${fmt(balances[receipt as 'kVARA' | 'kUSDT' | 'kUSDC'])} ${receipt}` : account ? '…' : '—'} loading={!!account && !balances} />
            <IRow icon={<Star size={14} strokeWidth={1.5} />} k="APY" v={apyBps !== null ? bpsToPercent(apyBps) : '…'} accent loading={!stats} />
            <IRow icon={<Coins size={14} strokeWidth={1.5} />} k="You receive" v={`${fmt(out.main)} ${tab === 'Stake' ? receipt : deposit}`} />
            {tab === 'Unstake' && out.fee > 0n && parsed ? <div style={{ fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--text-3)', textAlign: 'right', marginTop: -6 }}>fee {fmt(out.fee, 4)} {deposit}</div> : null}
          </div>
          <Button size="xl" block disabled={!!account && !validation.ok} loading={busy} onClick={act}>
            {busy ? 'Confirm in your wallet…' : label}
          </Button>
          {tx.stage === 'finalized' && tx.result && (
            <div style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--text-3)', marginTop: 10, textAlign: 'center' }}>tx {tx.result.hash.slice(0, 10)}… included in block {tx.result.blockNumber?.toLocaleString('en-US')}</div>
          )}
        </Bento>
      </div>

      <div style={{ display: 'flex', flexDirection: 'column', gap: 14 }}>
        <div className="ap-grid-2">
          <div style={{ borderRadius: 24, background: 'var(--grad-primary)', padding: '18px 18px 20px' }}>
            <div className="eyebrow" style={{ color: 'rgba(255,255,255,.85)' }}>TVL · 3 vaults</div>
            <div style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 27, color: '#fff', marginTop: 6 }}>{stats ? formatCompactUsd(stats.tvlUsd) : '…'}</div>
          </div>
          <Bento variant="app" pad={18}>
            <Eyebrow>{receipt} APY</Eyebrow>
            <div style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 27, marginTop: 6, color: 'var(--text-1)' }}>{apyBps !== null ? bpsToPercent(apyBps) : '…'}</div>
          </Bento>
        </div>
        <Bento variant="app" pad={20}>
          <div style={{ display: 'flex', justifyContent: 'space-between', alignItems: 'center' }}>
            <span style={{ fontSize: 15, fontWeight: 600, fontFamily: 'var(--font-display)' }}>Next compound</span>
            <Badge size="sm" tone="info">era {stats ? stats.era.toLocaleString('en-US') : '—'}</Badge>
          </div>
          <div style={{ display: 'flex', alignItems: 'baseline', gap: 8, marginTop: 10 }}>
            <span style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 25 }}>{stats ? formatCountdown(stats.eraEndsAt - now) : '…'}</span>
            <span style={{ fontFamily: 'var(--font-mono)', fontSize: 11.5, color: 'var(--text-3)' }}>rate {stats ? formatRate(stats.rate) : '—'}</span>
          </div>
          <div style={{ position: 'relative', margin: '26px 4px 6px' }}>
            <div style={{ position: 'absolute', left: 0, right: 0, top: 5, height: 2, background: 'var(--ink-700)', borderRadius: 2 }} />
            <div style={{ display: 'flex', justifyContent: 'space-between', position: 'relative' }}>
              {(stats?.rateHistory ?? [0, 1, 2].map((i) => ({ era: i, rate: 0n }))).map((h, i, arr) => {
                const hot = i === arr.length - 1;
                return (
                  <div key={h.era} style={{ textAlign: 'center', position: 'relative' }}>
                    {hot && stats ? <span style={{ position: 'absolute', left: '50%', transform: 'translateX(-50%)', top: -26, fontFamily: 'var(--font-mono)', fontSize: 10.5, background: '#fff', border: '1px solid var(--accent-line)', borderRadius: 7, padding: '2px 7px', color: 'var(--fx-indigo)', whiteSpace: 'nowrap' }}>{formatRate(h.rate)}</span> : null}
                    <div style={{ width: 12, height: 12, borderRadius: 99, margin: '0 auto', background: hot ? 'var(--fx-indigo)' : 'var(--ink-600)', border: '2px solid #fff' }} />
                    <div style={{ fontFamily: 'var(--font-mono)', fontSize: 10, color: 'var(--text-3)', marginTop: 7 }}>era {stats ? h.era.toLocaleString('en-US') : '—'}</div>
                  </div>
                );
              })}
            </div>
          </div>
        </Bento>
        {statsError && <p role="alert" style={{ fontSize: 13, color: 'var(--danger)', lineHeight: 1.5 }}>Protocol stats unavailable: {statsError}</p>}
        <p style={{ fontFamily: 'var(--font-mono)', fontSize: 11, color: 'var(--text-3)', lineHeight: 1.6 }}>
          {adapter.simulated ? 'Simulated protocol: figures are illustrative and nothing is sent on chain.' : 'Yield is embedded in the rate. Staking involves slashing risk.'}
        </p>
      </div>
    </div>
  );
}
