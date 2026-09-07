import { ONE_VARA } from '@/domain/protocol';
import { accrueRate } from '@/domain/math';
import { parseRate } from '@/domain/format';

/**
 * The kVARA pool's published figures: the simulation runs on them, and the chain adapter shows
 * them until a pool program id is configured (after that everything comes from the program).
 * The rate accrues continuously from T0 at the pool APY, so a receipt is visibly worth more
 * every second it is held.
 */
export const T0 = Date.UTC(2026, 8, 1);
export const BASE_ERA = 4182;
export const ERA_MS = 12 * 60 * 60 * 1000;
export const BASE_RATE = parseRate('1.0482');
/** kVARA compounds native staking rewards; the pool realises ~35% APY. */
export const VARA_APY_BPS = 3500n;
/** 918.27M VARA at $0.0004258 = $391K. */
export const STAKED_VARA = 918_270_000n * ONE_VARA;
export const VARA_PRICE_USD = 0.0004258;

/** Whole seconds since T0, never negative, so rates are stable within a second. */
export function elapsedMs(now: number): number {
  return Math.max(0, Math.floor((now - T0) / 1000) * 1000);
}

export function varaRateAt(now: number): bigint {
  return accrueRate(BASE_RATE, VARA_APY_BPS, elapsedMs(now));
}

export function varaEra(now: number): { era: number; endsAt: number } {
  const elapsed = elapsedMs(now);
  const era = BASE_ERA + Math.floor(elapsed / ERA_MS);
  return { era, endsAt: T0 + (era - BASE_ERA + 1) * ERA_MS };
}

export function varaRateHistory(now: number) {
  const { era } = varaEra(now);
  const rate = varaRateAt(now);
  const eraRate = (e: number) => accrueRate(BASE_RATE, VARA_APY_BPS, Math.max(0, (e - BASE_ERA) * ERA_MS));
  return [era - 2, era - 1, era].map((e) => ({ era: e, rate: e === era ? rate : eraRate(e) }));
}

export const VARA_TVL_USD = (Number(STAKED_VARA) / Number(ONE_VARA)) * VARA_PRICE_USD;
