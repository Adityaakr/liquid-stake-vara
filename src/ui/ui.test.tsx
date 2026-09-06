import { describe, expect, it } from 'vitest';
import { normalizeAmountInput } from './AmountField';
import { recallAccount } from '@/chain/wallet';
import { kVaraToVara, sharesToAssets, validateAmount } from '@/domain/math';

describe('amount input normalization', () => {
  it('treats grouped commas as thousands and a lone comma as a decimal', () => {
    expect(normalizeAmountInput('1,000')).toBe('1000');
    expect(normalizeAmountInput('1,000.5')).toBe('1000.5');
    expect(normalizeAmountInput('1,5')).toBe('1.5');
    expect(normalizeAmountInput('12')).toBe('12');
  });
});

describe('remembered account', () => {
  it('drops malformed storage instead of crashing', () => {
    localStorage.setItem('vale.wallet.v1', JSON.stringify({ nope: true }));
    expect(recallAccount()).toBeNull();
    expect(localStorage.getItem('vale.wallet.v1')).toBeNull();
    localStorage.setItem('vale.wallet.v1', '{not json');
    expect(recallAccount()).toBeNull();
  });
});

describe('math guards', () => {
  it('rejects zero rate and share price', () => {
    expect(() => kVaraToVara(1n, 0n)).toThrow(RangeError);
    expect(() => sharesToAssets(1n, 0n)).toThrow(RangeError);
  });
  it('skips the insufficient check while balance is loading', () => {
    expect(validateAmount('5', 5n, null)).toEqual({ ok: true });
  });
});
