import { BPS, INSTANT_UNSTAKE_FEE_BPS, RATE_SCALE } from './protocol';

/**
 * All amounts are bigint in base units (planck for VARA, 1e-6 for stables).
 * The exchange rate is scaled by RATE_SCALE. Kit semantics:
 *   mint:   kVARA = VARA / rate
 *   redeem: VARA     = kVARA × rate
 */

export function varaToKVara(vara: bigint, rate: bigint): bigint {
  if (rate <= 0n) throw new RangeError('rate must be positive');
  return (vara * RATE_SCALE) / rate;
}

export function kVaraToVara(kvara: bigint, rate: bigint): bigint {
  if (rate <= 0n) throw new RangeError('rate must be positive');
  return (kvara * rate) / RATE_SCALE;
}

export function instantUnstakeFee(varaOut: bigint): bigint {
  return (varaOut * INSTANT_UNSTAKE_FEE_BPS) / BPS;
}

/** Instant exit: redeem at rate, minus the 0.3% fee. */
export function instantUnstakeOut(kvara: bigint, rate: bigint): { gross: bigint; fee: bigint; net: bigint } {
  const gross = kVaraToVara(kvara, rate);
  const fee = instantUnstakeFee(gross);
  return { gross, fee, net: gross - fee };
}

/** Native exit: full rate, no fee, 7 day wait. */
export function nativeUnstakeOut(kvara: bigint, rate: bigint): bigint {
  return kVaraToVara(kvara, rate);
}

/** Vault shares (4626 style): shares = assets / sharePrice, assets = shares × sharePrice. */
export function assetsToShares(assets: bigint, sharePrice: bigint): bigint {
  if (sharePrice <= 0n) throw new RangeError('sharePrice must be positive');
  return (assets * RATE_SCALE) / sharePrice;
}
export function sharesToAssets(shares: bigint, sharePrice: bigint): bigint {
  if (sharePrice <= 0n) throw new RangeError('sharePrice must be positive');
  return (shares * sharePrice) / RATE_SCALE;
}

/** Simple projection used for "potential earning" rows. apyBps is annual, compounding ignored on purpose (display only). */
export function projectedYield(principal: bigint, apyBps: bigint, days: number): bigint {
  return (principal * apyBps * BigInt(Math.round(days))) / (BPS * 365n);
}

export const YEAR_MS = 365n * 24n * 60n * 60n * 1000n;

/**
 * A rate (or share price) `elapsedMs` later at `apyBps`, simple interest between checkpoints.
 * Rates only move forward: a negative elapsed time returns the rate unchanged.
 */
export function accrueRate(rate: bigint, apyBps: bigint, elapsedMs: number): bigint {
  if (elapsedMs <= 0) return rate;
  return rate + (rate * apyBps * BigInt(Math.floor(elapsedMs))) / (BPS * YEAR_MS);
}

/**
 * Project a rate read at `at` to `now`. Rewards vest linearly, so between two reads the rate grows
 * at the reported APY, but never past the end of the current tranche (`vestingEndsAt`, 0 = none).
 */
export function projectRate(rate: bigint, apyBps: bigint, at: number, now: number, vestingEndsAt: number): bigint {
  return accrueRate(rate, apyBps, Math.min(now, vestingEndsAt) - at);
}

export type AmountValidation = { ok: true } | { ok: false; reason: 'empty' | 'zero' | 'insufficient' | 'invalid' };

/** `balance` null means the balance is still loading: the insufficient check is skipped. */
export function validateAmount(raw: string, parsed: bigint | null, balance: bigint | null): AmountValidation {
  if (raw.trim() === '') return { ok: false, reason: 'empty' };
  if (parsed === null) return { ok: false, reason: 'invalid' };
  if (parsed <= 0n) return { ok: false, reason: 'zero' };
  if (balance !== null && parsed > balance) return { ok: false, reason: 'insufficient' };
  return { ok: true };
}
