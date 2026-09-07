import { describe, expect, it } from 'vitest';
import { accrueRate, assetsToShares, instantUnstakeOut, nativeUnstakeOut, projectRate, projectedYield, sharesToAssets, kVaraToVara, validateAmount, varaToKVara } from './math';
import { formatRate, formatUnits, formatVara, parseRate, parseUnits, parseVara, shortAddress, formatCountdown } from './format';
import { ONE_VARA, RATE_SCALE } from './protocol';

const RATE = parseRate('1.0482');

describe('exchange rate math', () => {
  it('mints kVARA = VARA / rate', () => {
    const out = varaToKVara(100n * ONE_VARA, RATE);
    expect(formatVara(out, 4)).toBe('95.4016');
  });
  it('redeems VARA = kVARA × rate and round trips within dust', () => {
    const kvara = varaToKVara(100n * ONE_VARA, RATE);
    const back = kVaraToVara(kvara, RATE);
    expect(100n * ONE_VARA - back).toBeLessThan(10n);
  });
  it('rate 1.0 is identity', () => {
    expect(varaToKVara(7n * ONE_VARA, RATE_SCALE)).toBe(7n * ONE_VARA);
  });
  it('rejects non positive rate', () => {
    expect(() => varaToKVara(1n, 0n)).toThrow(RangeError);
  });
});

describe('exits', () => {
  it('instant exit charges exactly 0.3% of gross', () => {
    const { gross, fee, net } = instantUnstakeOut(1000n * ONE_VARA, RATE_SCALE);
    expect(gross).toBe(1000n * ONE_VARA);
    expect(fee).toBe(3n * ONE_VARA);
    expect(net).toBe(997n * ONE_VARA);
  });
  it('native exit pays full rate with no fee', () => {
    expect(nativeUnstakeOut(1000n * ONE_VARA, RATE)).toBe(kVaraToVara(1000n * ONE_VARA, RATE));
  });
  it('instant net is always below native for a positive amount', () => {
    const t = 12345n * ONE_VARA;
    expect(instantUnstakeOut(t, RATE).net).toBeLessThan(nativeUnstakeOut(t, RATE));
  });
});

describe('vault shares', () => {
  it('converts assets to shares and back', () => {
    const price = parseRate('1.0261');
    const shares = assetsToShares(500_000_000n, price);
    expect(sharesToAssets(shares, price)).toBeLessThanOrEqual(500_000_000n);
    expect(500_000_000n - sharesToAssets(shares, price)).toBeLessThan(5n);
  });
});

describe('projection', () => {
  it('14.2% for a year on 1000 VARA is 142 VARA', () => {
    expect(formatVara(projectedYield(1000n * ONE_VARA, 1420n, 365))).toBe('142.00');
  });
  it('projects a rate between reads but never past the end of the vesting tranche', () => {
    const at = 1_000_000;
    const day = 86_400_000;
    expect(projectRate(RATE_SCALE, 3650n, at, at + day, at + 7 * day)).toBe(accrueRate(RATE_SCALE, 3650n, day));
    expect(projectRate(RATE_SCALE, 3650n, at, at + 30 * day, at + 7 * day)).toBe(accrueRate(RATE_SCALE, 3650n, 7 * day));
    expect(projectRate(RATE_SCALE, 0n, at, at + day, 0)).toBe(RATE_SCALE);
    expect(projectRate(RATE_SCALE, 3650n, at, at + day, Infinity)).toBe(accrueRate(RATE_SCALE, 3650n, day));
  });
});

describe('amount validation', () => {
  it('flags empty, invalid, zero, insufficient', () => {
    expect(validateAmount('', null, 10n)).toEqual({ ok: false, reason: 'empty' });
    expect(validateAmount('abc', null, 10n)).toEqual({ ok: false, reason: 'invalid' });
    expect(validateAmount('0', 0n, 10n)).toEqual({ ok: false, reason: 'zero' });
    expect(validateAmount('11', 11n, 10n)).toEqual({ ok: false, reason: 'insufficient' });
    expect(validateAmount('10', 10n, 10n)).toEqual({ ok: true });
  });
});

describe('formatting', () => {
  it('parses decimals into base units', () => {
    expect(parseVara('1')).toBe(ONE_VARA);
    expect(parseVara('0.5')).toBe(ONE_VARA / 2n);
    expect(parseVara('1,240.52')).toBe(1240n * ONE_VARA + (52n * ONE_VARA) / 100n);
    expect(parseVara('.')).toBeNull();
    expect(parseVara('1e5')).toBeNull();
    expect(parseUnits('0.1234567', 6)).toBeNull();
  });
  it('formats with grouping and truncation, never rounding up', () => {
    expect(formatVara(1240n * ONE_VARA + (529n * ONE_VARA) / 1000n)).toBe('1,240.52');
    expect(formatUnits(0n, 12)).toBe('0.00');
    expect(formatUnits(-5n * ONE_VARA, 12)).toBe('-5.00');
  });
  it('rates round trip', () => {
    expect(formatRate(parseRate('1.0482'))).toBe('1.0482');
  });
  it('shortens addresses and formats countdowns', () => {
    expect(shortAddress('kGj1akEAemmGoVyFqeHVSNUyUj1mJp3gazUYy7Zs2p7BsgT88')).toBe('kGj1ak…gT88');
    expect(formatCountdown(6 * 86_400_000 + 23 * 3_600_000)).toBe('6d 23h');
    expect(formatCountdown(0)).toBe('now');
  });
});
