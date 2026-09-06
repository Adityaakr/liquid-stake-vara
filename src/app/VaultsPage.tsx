import { useMemo, useState } from 'react';
import { useNavigate, useOutletContext } from 'react-router';
import { ChevronRight } from 'lucide-react';
import { AmountField, Badge, Bento, Button, Dialog, Meter, Stat, TokenIcon, type TokenSymbol } from '@/ui';
import { useStore } from '@/chain/store';
import type { DepositAsset } from '@/chain/types';
import { accrueRate, assetsToShares, sharesToAssets, validateAmount } from '@/domain/math';
import { bpsToPercent, formatCompactUsd, formatRate, formatUnits, parseUnits } from '@/domain/format';
import { STABLE_DECIMALS, VARA_DECIMALS, type VaultAsset } from '@/domain/protocol';
import { Row, useNow } from './bits';
import type { AppOutlet } from './AppLayout';

type Card = {
  asset: DepositAsset;
  deposit: TokenSymbol;
  receipt: TokenSymbol;
  decimals: number;
  apyBps: bigint;
  /** receipt -> asset rate at `at`, scaled 1e9 */
  rate: bigint;
  at: number;
  tvlUsd: number;
  utilBps: bigint | null;
  wallet: bigint;
  receipts: bigint;
  principal: bigint;
};

const trimZeros = (s: string) => s.replace(/,/g, '').replace(/\.?0+$/, '');
const fmt = (v: bigint, decimals: number, dp = 2) => formatUnits(v, decimals, dp);

/** Live value of a receipt balance right now: the rate keeps accruing between stat refreshes. */
function liveValue(card: Card, now: number): bigint {
  return sharesToAssets(card.receipts, accrueRate(card.rate, card.apyBps, now - card.at));
}

function VaultCard({ card }: { card: Card }) {
  const { openWallet } = useOutletContext<AppOutlet>();
  const nav = useNavigate();
  const { stats, balances, account, adapter, run, tx } = useStore();
  const now = useNow(1000);
  const [open, setOpen] = useState(false);
  const [amt, setAmt] = useState('');
  const { asset, deposit: dep, receipt: rec, decimals } = card;
  const parsed = useMemo(() => parseUnits(amt, decimals), [amt, decimals]);
  const v = validateAmount(amt, parsed, balances ? card.wallet : null);
  const apy = bpsToPercent(card.apyBps);
  const busy = tx.stage === 'broadcast';
  const util = card.utilBps === null ? null : Number(card.utilBps) / 100;
  const rateNow = accrueRate(card.rate, card.apyBps, now - card.at);
  const value = liveValue(card, now);
  const earned = value > card.principal ? value - card.principal : 0n;
  const inYear = (x: bigint) => x + (x * card.apyBps) / 10_000n;
  const hasPosition = card.receipts > 0n;

  const doDep = async () => {
    if (!account) { setOpen(false); openWallet(); return; }
    if (!v.ok || !parsed) return;
    const a = parsed;
    const receipts = assetsToShares(a, rateNow);
    const ok = asset === 'VARA'
      ? await run('Stake', (addr) => adapter.stake(addr, a), { title: `Staked ${fmt(a, decimals)} VARA`, detail: `You hold ${fmt(receipts, decimals)} kVARA. Rewards compound into the rate every era.` })
      : await run(`Deposit ${dep}`, (addr) => adapter.depositVault(addr, asset, a), { title: `Deposited ${fmt(a, decimals)} ${dep}`, detail: `You hold ${fmt(receipts, decimals)} ${rec}. Interest accrues to the share price every second.` });
    if (ok) { setOpen(false); setAmt(''); }
  };

  return (
    <Bento variant="app" pad={26}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <TokenIcon token={rec} size={38} />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <span style={{ fontFamily: 'var(--font-mono)', fontWeight: 600, fontSize: 16, letterSpacing: '-0.01em' }}>{rec} / {asset}</span>
            <span style={{ fontSize: 11.5, color: 'var(--text-3)' }}>{asset === 'VARA' ? 'liquid staking pool' : 'stable vault'}</span>
          </div>
        </div>
        {util === null ? <Badge tone="accent" size="sm" dot>liquid staking</Badge> : <Badge tone={util < 80 ? 'ok' : 'warn'} size="sm" dot>{util < 80 ? 'healthy' : 'near kink'}</Badge>}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 14, margin: '22px 0 16px' }}>
        <Stat label={asset === 'VARA' ? 'Staking APY' : 'Deposit APY'} value={apy} size="sm" gradient loading={!stats} />
        <Stat label="TVL" value={formatCompactUsd(card.tvlUsd)} size="sm" loading={!stats} />
        <Stat label={asset === 'VARA' ? 'Rate' : 'Share price'} value={formatRate(rateNow, 6)} size="sm" mono loading={!stats} />
      </div>
      {util !== null ? <Meter label="Utilization" value={util} marker={80} tone={util < 80 ? 'grad' : 'danger'} /> : (
        <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
          {['instant exit 0.3%', 'native unbond 7 days', 'no lockup on kVARA'].map((t) => <span key={t} style={{ fontSize: 12, color: 'var(--text-2)', background: '#fff', border: '1px solid var(--fx-mist)', borderRadius: 99, padding: '4px 10px' }}>{t}</span>)}
        </div>
      )}
      <div style={{ margin: '12px 0 20px' }}>
        <Row k="Your position" v={account && balances ? `${fmt(card.receipts, decimals)} ${rec} ≈ ${fmt(value, decimals)} ${asset}` : '—'} />
        <Row k="Earned so far" v={account && balances ? (hasPosition ? `+${fmt(earned, decimals, 6)} ${asset}` : `0 ${asset}`) : '—'} accent={hasPosition} />
        <Row k="Worth in 12 months" v={account && balances && hasPosition ? `≈ ${fmt(inYear(value), decimals)} ${asset}` : `${apy} on every ${asset}`} />
      </div>
      <div style={{ marginTop: 'auto', display: 'flex', gap: 10 }}>
        <Button block size="lg" onClick={() => setOpen(true)}>{asset === 'VARA' ? 'Stake VARA' : `Deposit ${dep}`}</Button>
        {asset === 'VARA' && hasPosition && <Button size="lg" variant="secondary" onClick={() => nav('/app')}>Unstake</Button>}
      </div>
      <Dialog
        open={open}
        onClose={() => setOpen(false)}
        title={asset === 'VARA' ? 'Stake VARA' : `Deposit ${dep}`}
        footer={
          <>
            <Button variant="ghost" onClick={() => setOpen(false)}>Cancel</Button>
            <Button disabled={!!account && !v.ok} loading={busy} onClick={doDep}>{account ? (asset === 'VARA' ? 'Confirm stake' : 'Confirm deposit') : 'Connect wallet'}</Button>
          </>
        }
      >
        <AmountField
          label={asset === 'VARA' ? 'You stake' : 'You deposit'}
          token={dep}
          balance={balances ? fmt(card.wallet, decimals) : undefined}
          value={amt}
          onChange={setAmt}
          onMax={balances ? () => setAmt(trimZeros(fmt(card.wallet, decimals, decimals))) : undefined}
          fiat={parsed ? `→ ${fmt(assetsToShares(parsed, rateNow), decimals)} ${rec} · ≈ ${fmt(inYear(parsed), decimals)} ${asset} in 12 months` : ''}
          hint={`${apy} APY, paid through the rate. Your ${rec} stays liquid and transferable.`}
          error={amt && !v.ok && v.reason !== 'empty' ? (v.reason === 'insufficient' ? 'Not enough balance' : 'Enter a valid amount') : undefined}
        />
      </Dialog>
    </Bento>
  );
}

function FlowChip({ tok, label }: { tok?: TokenSymbol; label: string }) {
  return (
    <span style={{ display: 'inline-flex', alignItems: 'center', gap: 8, background: '#fff', border: '1px solid var(--fx-mist)', borderRadius: 99, padding: '8px 14px 8px 9px', whiteSpace: 'nowrap' }}>
      {tok ? <TokenIcon token={tok} size={20} /> : null}
      <span style={{ fontSize: 12.5, fontWeight: 500, color: 'var(--text-1)' }}>{label}</span>
    </span>
  );
}
const Arrow = () => <ChevronRight size={15} strokeWidth={1.5} style={{ color: 'var(--fx-indigo)', flexShrink: 0 }} />;

export function VaultsPage() {
  const { notify, stats, balances } = useStore();
  const now = useNow(1000);
  const at = stats?.at ?? now;
  const stable = (asset: VaultAsset): Card => ({
    asset,
    deposit: `w${asset}` as TokenSymbol,
    receipt: `k${asset}` as TokenSymbol,
    decimals: STABLE_DECIMALS,
    apyBps: stats?.vaultApyBps[asset] ?? 0n,
    rate: stats?.vaultSharePrice[asset] ?? 1_000_000_000n,
    at,
    tvlUsd: stats?.vaultTvlUsd[asset] ?? 0,
    utilBps: stats?.vaultUtilizationBps[asset] ?? 0n,
    wallet: balances?.[`w${asset}`] ?? 0n,
    receipts: balances?.[`k${asset}`] ?? 0n,
    principal: balances?.principal[asset] ?? 0n,
  });
  const vara: Card = {
    asset: 'VARA',
    deposit: 'VARA',
    receipt: 'kVARA',
    decimals: VARA_DECIMALS,
    apyBps: stats?.stakeApyBps ?? 0n,
    rate: stats?.rate ?? 1_000_000_000n,
    at,
    tvlUsd: stats ? (Number(stats.totalStakedVara) / 10 ** VARA_DECIMALS) * stats.varaPriceUsd : 0,
    utilBps: null,
    wallet: balances?.VARA ?? 0n,
    receipts: balances?.kVARA ?? 0n,
    principal: balances?.principal.VARA ?? 0n,
  };
  return (
    <div style={{ display: 'flex', flexDirection: 'column', gap: 20 }}>
      <div className="ap-vaults">
        <VaultCard card={vara} />
        <VaultCard card={stable('USDT')} />
        <VaultCard card={stable('USDC')} />
      </div>
      <Bento variant="app" pad={26}>
        <div className="ap-yield">
          <div>
            <div style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 20 }}>Where the yield comes from</div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, margin: '16px 0 14px', flexWrap: 'wrap' }}>
              <FlowChip tok="kVARA" label="kVARA collateral" /><Arrow />
              <FlowChip label="loopers borrow stables" /><Arrow />
              <FlowChip label="interest accrues" /><Arrow />
              <FlowChip tok="kUSDT" label="share price rises" />
            </div>
            <div style={{ display: 'flex', gap: 8, marginTop: 2, flexWrap: 'wrap' }}>
              {['max LTV 50%', 'liq. threshold 65%', 'penalty 8%', 'kink 80%'].map((t) => (
                <span key={t} style={{ fontFamily: 'var(--font-body)', fontSize: 12.5, color: 'var(--text-2)', background: '#fff', borderRadius: 99, padding: '5px 12px' }}>{t}</span>
              ))}
            </div>
          </div>
          <Button variant="secondary" onClick={() => notify({ tone: 'info', title: 'Borrowing opens in v1.1', detail: 'Loop kVARA → stables → VARA from a single screen.' })}>Borrow against kVARA</Button>
        </div>
      </Bento>
    </div>
  );
}
