import { useMemo, useState } from 'react';
import { useOutletContext } from 'react-router';
import { ChevronRight } from 'lucide-react';
import { AmountField, Badge, Bento, Button, Dialog, Stat, TokenIcon, type TokenSymbol } from '@/ui';
import { useStore } from '@/chain/store';
import type { DepositAsset } from '@/chain/types';
import { accrueRate, assetsToShares, sharesToAssets, validateAmount } from '@/domain/math';
import { bpsToPercent, formatCompactUsd, formatCountdown, formatRate, formatUnits, parseUnits } from '@/domain/format';
import { BPS, INSTANT_UNSTAKE_FEE_BPS, STABLE_DECIMALS, UNBONDING_DAYS, VARA_DECIMALS, type VaultAsset } from '@/domain/protocol';
import { Row, useNow } from './bits';
import type { AppOutlet } from './AppLayout';

type Card = {
  asset: DepositAsset;
  receipt: TokenSymbol;
  decimals: number;
  apyBps: bigint;
  /** receipt -> asset rate at `at`, scaled 1e9 */
  rate: bigint;
  at: number;
  tvlUsd: number;
  feeBps: bigint;
  unbondSecs: number;
  minDeposit: bigint;
  paused: boolean;
  live: boolean;
  wallet: bigint;
  receipts: bigint;
  principal: bigint | null;
};
type ExitPath = 'instant' | 'unbond';

const trimZeros = (s: string) => s.replace(/,/g, '').replace(/\.?0+$/, '');
const fmt = (v: bigint, decimals: number, dp = 2) => formatUnits(v, decimals, dp);
const REASON: Record<string, string> = { invalid: 'Enter a valid amount', zero: 'Amount must be more than zero', insufficient: 'Not enough balance', min: 'Below the vault minimum' };

function periodLabel(secs: number): string {
  if (secs >= 86_400) return `${Math.round(secs / 86_400)} day${secs >= 2 * 86_400 ? 's' : ''}`;
  if (secs >= 3600) return `${Math.round(secs / 3600)} h`;
  return `${Math.max(1, Math.round(secs / 60))} min`;
}

function ExitOption({ title, sub, active, onClick }: { title: string; sub: string; active: boolean; onClick: () => void }) {
  return (
    <button type="button" className="ap-exit" aria-pressed={active} onClick={onClick}>
      <div className="ap-exit-t">{title}</div>
      <div className="ap-exit-s">{sub}</div>
    </button>
  );
}

function VaultCard({ card }: { card: Card }) {
  const { openWallet } = useOutletContext<AppOutlet>();
  const { stats, balances, unbonding, account, adapter, run, tx } = useStore();
  const now = useNow(1000);
  const [dialog, setDialog] = useState<'deposit' | 'withdraw' | null>(null);
  const [amt, setAmt] = useState('');
  const [path, setPath] = useState<ExitPath>('instant');
  const { asset, receipt: rec, decimals } = card;
  const isVara = asset === 'VARA';
  const parsed = useMemo(() => parseUnits(amt, decimals), [amt, decimals]);
  const apy = bpsToPercent(card.apyBps);
  const busy = tx.stage === 'broadcast';
  const rateNow = accrueRate(card.rate, card.apyBps, now - card.at);
  const value = sharesToAssets(card.receipts, rateNow);
  const earned = card.principal !== null && value > card.principal ? value - card.principal : 0n;
  const inYear = (x: bigint) => x + (x * card.apyBps) / 10_000n;
  const hasPosition = card.receipts > 0n;
  const myUnbonds = unbonding.filter((u) => u.asset === asset);

  const depV = validateAmount(amt, parsed, balances ? card.wallet : null);
  const depError = amt && !depV.ok && depV.reason !== 'empty' ? REASON[depV.reason] : amt && parsed !== null && parsed < card.minDeposit ? REASON.min : undefined;
  const depOk = depV.ok && parsed !== null && parsed >= card.minDeposit;
  const wV = validateAmount(amt, parsed, balances ? card.receipts : null);
  const wError = amt && !wV.ok && wV.reason !== 'empty' ? REASON[wV.reason] : undefined;
  const preview = (() => {
    if (!parsed) return null;
    const assets = sharesToAssets(parsed, rateNow);
    const fee = path === 'instant' ? (assets * card.feeBps) / BPS : 0n;
    return { assets, fee, net: assets - fee };
  })();

  const close = () => { setDialog(null); setAmt(''); };

  const doDeposit = async () => {
    if (!account) { close(); openWallet(); return; }
    if (!depOk || !parsed) return;
    const a = parsed;
    const receipts = assetsToShares(a, rateNow);
    const ok = isVara
      ? await run('Stake', (addr) => adapter.stake(addr, a), { title: `Staked ${fmt(a, decimals)} VARA`, detail: `You hold ${fmt(receipts, decimals)} kVARA. Rewards compound into the rate every era.` })
      : await run(`Deposit ${asset}`, (addr) => adapter.depositVault(addr, asset, a), (r) => ({ title: `Deposited ${fmt(a, decimals)} ${asset}`, detail: `You hold ${fmt((r as { shares?: bigint }).shares ?? receipts, decimals)} more ${rec}. Yield accrues to the rate every second.` }));
    if (ok) close();
  };

  const doWithdraw = async () => {
    if (!account) { close(); openWallet(); return; }
    if (!wV.ok || !parsed) return;
    const s = parsed;
    let ok = false;
    if (isVara) {
      ok = path === 'instant'
        ? await run('Instant unstake', (addr) => adapter.unstakeInstant(addr, s), { title: 'Unstaked instantly', detail: `${fmt(preview?.net ?? 0n, decimals)} VARA received · ${bpsToPercent(card.feeBps, 2)} fee applied.` })
        : await run('Native unbond', (addr) => adapter.unstakeNative(addr, s), { title: 'Unbond started', detail: `${fmt(preview?.assets ?? 0n, decimals)} VARA claimable in ${periodLabel(card.unbondSecs)} at full rate.` });
    } else {
      ok = path === 'instant'
        ? await run(`Withdraw ${asset}`, (addr) => adapter.redeemVault(addr, asset, s), (r) => ({ title: `Withdrew ${fmt((r as { net?: bigint }).net ?? preview?.net ?? 0n, decimals)} ${asset}`, detail: `${bpsToPercent(card.feeBps, 2)} instant exit fee applied.` }))
        : await run(`Unbond ${asset}`, (addr) => adapter.unbondVault(addr, asset, s), (r) => ({ title: 'Unbond started', detail: `${fmt((r as { amount?: bigint }).amount ?? preview?.assets ?? 0n, decimals)} ${asset} claimable in ${periodLabel(card.unbondSecs)} at full rate.` }));
    }
    if (ok) close();
  };


  const disabled = !card.live || card.paused;

  return (
    <Bento variant="app" pad={26}>
      <div style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, flexWrap: 'wrap' }}>
        <div style={{ display: 'flex', alignItems: 'center', gap: 10 }}>
          <TokenIcon token={rec} size={38} />
          <div style={{ display: 'flex', flexDirection: 'column', gap: 2 }}>
            <span style={{ fontFamily: 'var(--font-mono)', fontWeight: 600, fontSize: 16, letterSpacing: '-0.01em' }}>{rec} / {asset}</span>
            <span style={{ fontSize: 11.5, color: 'var(--text-3)' }}>{isVara ? 'liquid staking pool' : 'stable vault'}</span>
          </div>
        </div>
        {!card.live ? <Badge tone="info" size="sm">coming soon</Badge> : card.paused ? <Badge tone="warn" size="sm">paused</Badge> : <Badge tone={isVara ? 'accent' : 'ok'} size="sm">{isVara ? 'liquid staking' : 'live'}</Badge>}
      </div>
      <div style={{ display: 'grid', gridTemplateColumns: '1fr 1fr 1fr', gap: 14, margin: '22px 0 16px' }}>
        <Stat label={isVara ? 'Staking APY' : 'Deposit APY'} value={card.live ? apy : '—'} size="sm" gradient loading={!stats} />
        <Stat label="TVL" value={card.live ? formatCompactUsd(card.tvlUsd) : '—'} size="sm" loading={!stats} />
        <Stat label={isVara ? 'Rate' : 'Share price'} value={card.live ? formatRate(rateNow, 6) : '—'} size="sm" mono loading={!stats} />
      </div>
      <div style={{ display: 'flex', gap: 8, flexWrap: 'wrap' }}>
        {[`instant exit ${bpsToPercent(card.feeBps, 2)}`, `unbond ${periodLabel(card.unbondSecs)}`, `no lockup on ${rec}`].map((t) => <span key={t} style={{ fontSize: 12, color: 'var(--text-2)', background: '#fff', border: '1px solid var(--fx-mist)', borderRadius: 99, padding: '4px 10px' }}>{t}</span>)}
      </div>
      <div style={{ margin: '12px 0 18px' }}>
        <Row k="Your position" v={account && balances ? `${fmt(card.receipts, decimals)} ${rec} ≈ ${fmt(value, decimals)} ${asset}` : '—'} />
        {card.principal !== null && <Row k="Earned so far" v={account && balances ? (hasPosition ? `+${fmt(earned, decimals, 6)} ${asset}` : `0 ${asset}`) : '—'} accent={hasPosition} />}
        <Row k="Worth in 12 months" v={!card.live ? 'after the validator integration' : account && balances && hasPosition ? `≈ ${fmt(inYear(value), decimals)} ${asset}` : `${apy} on every ${asset}`} />
        <Row k={`Wallet ${asset}`} v={account && balances ? `${fmt(card.wallet, decimals)} ${asset}` : '—'} />
      </div>
      <div style={{ marginTop: 'auto', display: 'flex', gap: 10, flexWrap: 'wrap' }}>
        <Button size="lg" style={{ flex: 1, minWidth: 140 }} disabled={disabled} onClick={() => { if (!account) { openWallet(); return; } setDialog('deposit'); }}>{isVara ? 'Stake VARA' : `Deposit ${asset}`}</Button>
        <Button size="lg" variant="secondary" style={{ flex: 1, minWidth: 140 }} disabled={disabled || !account || !hasPosition} onClick={() => setDialog('withdraw')}>{isVara ? 'Unstake' : 'Withdraw'}</Button>
      </div>
      {myUnbonds.length > 0 && (
        <div style={{ marginTop: 14, display: 'flex', flexDirection: 'column', gap: 8 }}>
          {myUnbonds.map((u) => {
            const ready = now >= u.claimableAt;
            return (
              <div key={u.id} style={{ display: 'flex', alignItems: 'center', justifyContent: 'space-between', gap: 10, background: '#fff', border: '1px solid var(--fx-mist)', borderRadius: 14, padding: '10px 12px', flexWrap: 'wrap' }}>
                <div style={{ display: 'flex', alignItems: 'center', gap: 8 }}>
                  <Badge tone={ready ? 'ok' : 'info'} size="sm">{ready ? 'Claimable' : 'Unbonding'}</Badge>
                  <span style={{ fontSize: 13, color: 'var(--text-2)' }}>{fmt(u.amount, decimals)} {asset}</span>
                </div>
                {ready ? (
                  <Button size="sm" loading={busy} onClick={() => run(`Claim ${asset}`, (addr) => adapter.claimUnbond(addr, asset, u.id), { title: `Claimed ${fmt(u.amount, decimals)} ${asset}` })}>Claim</Button>
                ) : (
                  <span style={{ fontFamily: 'var(--font-mono)', fontSize: 12, color: 'var(--text-3)' }}>claim in {formatCountdown(u.claimableAt - now)}</span>
                )}
              </div>
            );
          })}
        </div>
      )}

      <Dialog
        open={dialog === 'deposit'}
        onClose={close}
        title={isVara ? 'Stake VARA' : `Deposit ${asset}`}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Cancel</Button>
            <Button disabled={!!account && !depOk} loading={busy} onClick={doDeposit}>{account ? (isVara ? 'Confirm stake' : 'Confirm deposit') : 'Connect wallet'}</Button>
          </>
        }
      >
        <AmountField
          label={isVara ? 'You stake' : 'You deposit'}
          token={asset}
          balance={balances ? fmt(card.wallet, decimals) : undefined}
          value={amt}
          onChange={setAmt}
          onMax={balances ? () => setAmt(trimZeros(fmt(card.wallet, decimals, decimals))) : undefined}
          fiat={parsed ? `→ ${fmt(assetsToShares(parsed, rateNow), decimals)} ${rec} · ≈ ${fmt(inYear(parsed), decimals)} ${asset} in 12 months` : ''}
          hint={`${apy} APY, paid through the rate. Your ${rec} stays liquid and transferable.`}
          error={depError}
          disabled={busy}
        />
        {adapter.kind === 'gear' && !isVara && <p style={{ fontSize: 12.5, color: 'var(--text-3)', marginTop: 12, lineHeight: 1.6 }}>The first deposit asks for two signatures: an approval for the vault, then the deposit itself.</p>}
      </Dialog>

      <Dialog
        open={dialog === 'withdraw'}
        onClose={close}
        title={isVara ? 'Unstake VARA' : `Withdraw ${asset}`}
        footer={
          <>
            <Button variant="ghost" onClick={close}>Cancel</Button>
            <Button disabled={!!account && !wV.ok} loading={busy} onClick={doWithdraw}>{path === 'instant' ? (isVara ? 'Unstake now' : 'Withdraw now') : 'Start unbond'}</Button>
          </>
        }
      >
        <AmountField
          label="You redeem"
          token={rec}
          balance={balances ? fmt(card.receipts, decimals) : undefined}
          value={amt}
          onChange={setAmt}
          onMax={balances ? () => setAmt(trimZeros(fmt(card.receipts, decimals, decimals))) : undefined}
          fiat={preview ? `→ ${fmt(preview.net, decimals)} ${asset}${preview.fee > 0n ? ` (fee ${fmt(preview.fee, decimals, 4)})` : ''}` : ''}
          error={wError}
          disabled={busy}
        />
        <div style={{ display: 'flex', gap: 10, marginTop: 12 }}>
          <ExitOption title="Instant" sub={`${bpsToPercent(card.feeBps, 2)} fee · now`} active={path === 'instant'} onClick={() => setPath('instant')} />
          <ExitOption title="Unbond" sub={`free · ${periodLabel(card.unbondSecs)}`} active={path === 'unbond'} onClick={() => setPath('unbond')} />
        </div>
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
  const { stats, balances, adapter } = useStore();
  const now = useNow(1000);
  const at = stats?.at ?? now;
  const stable = (asset: VaultAsset): Card => {
    const v = stats?.vaults[asset];
    return {
      asset,
      receipt: `k${asset}` as TokenSymbol,
      decimals: STABLE_DECIMALS,
      apyBps: v?.apyBps ?? 0n,
      rate: v?.rate ?? 1_000_000_000n,
      at,
      tvlUsd: v?.tvlUsd ?? 0,
      feeBps: v?.instantFeeBps ?? INSTANT_UNSTAKE_FEE_BPS,
      unbondSecs: v?.unbondSecs ?? UNBONDING_DAYS * 86_400,
      minDeposit: v?.minDeposit ?? 0n,
      paused: v?.paused ?? false,
      live: adapter.deployed,
      wallet: balances?.[asset] ?? 0n,
      receipts: balances?.[`k${asset}`] ?? 0n,
      principal: balances?.principal ? balances.principal[asset] : null,
    };
  };
  const vara: Card = {
    asset: 'VARA',
    receipt: 'kVARA',
    decimals: VARA_DECIMALS,
    apyBps: stats?.stakeApyBps ?? 0n,
    rate: stats?.rate ?? 1_000_000_000n,
    at,
    tvlUsd: stats ? (Number(stats.totalStakedVara) / 10 ** VARA_DECIMALS) * stats.varaPriceUsd : 0,
    feeBps: INSTANT_UNSTAKE_FEE_BPS,
    unbondSecs: UNBONDING_DAYS * 86_400,
    minDeposit: 0n,
    paused: false,
    live: adapter.stakingLive,
    wallet: balances?.VARA ?? 0n,
    receipts: balances?.kVARA ?? 0n,
    principal: balances?.principal ? balances.principal.VARA : null,
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
            <div style={{ fontFamily: 'var(--font-display)', fontWeight: 600, fontSize: 20 }}>How the pools work</div>
            <div style={{ display: 'flex', alignItems: 'center', gap: 10, margin: '16px 0 14px', flexWrap: 'wrap' }}>
              <FlowChip tok="USDC" label="deposit" /><Arrow />
              <FlowChip tok="kUSDC" label="receive kUSDC at the current rate" /><Arrow />
              <FlowChip label="rate rises every second" /><Arrow />
              <FlowChip label="exit instantly or unbond" />
            </div>
            <div style={{ display: 'flex', gap: 8, marginTop: 2, flexWrap: 'wrap' }}>
              {['rate based receipts (no rebasing)', 'transferable kVARA, kUSDT and kUSDC', 'instant exit fee stays in the pool', 'demo tokens from the faucet'].map((t) => (
                <span key={t} style={{ fontFamily: 'var(--font-body)', fontSize: 12.5, color: 'var(--text-2)', background: '#fff', borderRadius: 99, padding: '5px 12px' }}>{t}</span>
              ))}
            </div>
          </div>
          <p style={{ fontSize: 13, color: 'var(--text-3)', lineHeight: 1.6, maxWidth: 320 }}>
            {adapter.kind === 'gear' ? 'USDC and USDT here are demo tokens on Vara mainnet with no monetary value. Yield on them is minted by the vault so the full flow can be tested end to end.' : 'Simulated protocol: figures are illustrative and nothing is sent on chain.'}
          </p>
        </div>
      </Bento>
    </div>
  );
}
