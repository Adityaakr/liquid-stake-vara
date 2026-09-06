import type { ReactNode } from 'react';
import { X } from 'lucide-react';

export type ToastTone = 'ok' | 'warn' | 'danger' | 'info';

export type ToastProps = { tone?: ToastTone; title: ReactNode; detail?: ReactNode; action?: ReactNode; onDismiss?: () => void; floating?: boolean };

export function Toast({ tone: _tone = 'ok', title, detail, action, onDismiss, floating }: ToastProps) {
  return (
    <div role="status" className={['t-toast', floating ? 't-toast-float' : ''].join(' ')}>
      <div style={{ flex: 1, display: 'flex', flexDirection: 'column', gap: 3 }}>
        <span style={{ fontSize: 'var(--fs-sm)', fontWeight: 600, color: 'var(--text-1)' }}>{title}</span>
        {detail ? <span style={{ fontSize: 'var(--fs-xs)', color: 'var(--text-2)', lineHeight: 1.5 }}>{detail}</span> : null}
        {action ?? null}
      </div>
      {onDismiss ? (
        <button type="button" onClick={onDismiss} aria-label="Dismiss" style={{ border: 'none', background: 'transparent', color: 'var(--text-3)', cursor: 'pointer', padding: 2, display: 'inline-flex' }}>
          <X size={14} strokeWidth={1.5} />
        </button>
      ) : null}
    </div>
  );
}
