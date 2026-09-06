import { useEffect, useId, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { ChevronDown } from 'lucide-react';
import { TokenBadge, TokenIcon, type TokenSymbol } from './Token';

export type AmountFieldProps = {
  label?: string;
  token?: TokenSymbol;
  /** When given, the token chip becomes a picker over these symbols. */
  tokens?: readonly TokenSymbol[];
  onToken?: (t: TokenSymbol) => void;
  balance?: string;
  value: string;
  onChange: (v: string) => void;
  onMax?: () => void;
  fiat?: ReactNode;
  hint?: ReactNode;
  error?: string;
  disabled?: boolean;
  style?: CSSProperties;
};

const DECIMAL = /^(\d+)?(\.\d*)?$/;
const GROUPED = /^\d{1,3}(,\d{3})+(\.\d*)?$/;

/** Accepts typed and pasted amounts: "1,000" and "1,000.5" are thousands groups, "1,5" alone is a locale decimal. */
export function normalizeAmountInput(v: string): string {
  const s = v.trim();
  if (!s.includes(',')) return s;
  if (GROUPED.test(s) || s.includes('.')) return s.replace(/,/g, '');
  return s.replace(',', '.');
}

function TokenPicker({ token, tokens, onToken, disabled }: { token: TokenSymbol; tokens: readonly TokenSymbol[]; onToken: (t: TokenSymbol) => void; disabled?: boolean }) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  const listId = useId();
  useEffect(() => {
    if (!open) return;
    const away = (e: MouseEvent) => { if (ref.current && !ref.current.contains(e.target as Node)) setOpen(false); };
    const esc = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    document.addEventListener('mousedown', away);
    document.addEventListener('keydown', esc);
    return () => { document.removeEventListener('mousedown', away); document.removeEventListener('keydown', esc); };
  }, [open]);
  return (
    <div ref={ref} style={{ position: 'relative' }}>
      <button type="button" className="t-amt-tok" aria-haspopup="listbox" aria-expanded={open} aria-controls={listId} aria-label={`Asset: ${token}`} disabled={disabled} onClick={() => setOpen((o) => !o)}>
        <TokenIcon token={token} size={24} />
        <span>{token}</span>
        <ChevronDown size={14} strokeWidth={2} style={{ color: 'var(--text-3)', transition: 'transform var(--dur-1) var(--ease-out)', transform: open ? 'rotate(180deg)' : undefined }} />
      </button>
      {open && (
        <div id={listId} role="listbox" aria-label="Choose asset" className="t-amt-menu">
          {tokens.map((t) => (
            <button key={t} type="button" role="option" aria-selected={t === token} className="t-amt-opt" onClick={() => { onToken(t); setOpen(false); }}>
              <TokenIcon token={t} size={22} />
              <span>{t}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}

export function AmountField({ label = 'Amount', token = 'VARA', tokens, onToken, balance, value, onChange, onMax, fiat, hint, error, disabled, style }: AmountFieldProps) {
  const errId = useId();
  return (
    <div className="t-amt" data-error={error ? 'true' : undefined} style={style}>
      <div className="t-amt-x">
        <span>{label}</span>
        {balance != null ? (
          <span>
            Balance <span style={{ fontFamily: 'var(--font-mono)', color: 'var(--text-2)' }}>{balance}</span>
          </span>
        ) : null}
      </div>
      <div style={{ display: 'flex', alignItems: 'center', gap: 12 }}>
        <input
          inputMode="decimal"
          autoComplete="off"
          placeholder="0.00"
          aria-label={label}
          aria-invalid={error ? true : undefined}
          aria-describedby={error ? errId : undefined}
          value={value}
          disabled={disabled}
          onChange={(e) => {
            const v = normalizeAmountInput(e.target.value);
            if (v === '' || DECIMAL.test(v)) onChange(v);
          }}
        />
        {onMax ? (
          <button type="button" className="t-amt-max" onClick={onMax} disabled={disabled}>
            MAX
          </button>
        ) : null}
        {tokens && onToken ? <TokenPicker token={token} tokens={tokens} onToken={onToken} disabled={disabled} /> : <TokenBadge token={token} size={26} />}
      </div>
      <div className="t-amt-x">
        <span id={errId} style={{ fontFamily: 'var(--font-mono)' }} className={error ? 't-amt-err' : undefined}>{error ?? fiat ?? ''}</span>
        <span>{hint ?? ''}</span>
      </div>
    </div>
  );
}
