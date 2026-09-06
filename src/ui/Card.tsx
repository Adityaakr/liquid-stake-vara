import type { CSSProperties, HTMLAttributes, ReactNode } from 'react';

export type CardProps = HTMLAttributes<HTMLDivElement> & {
  variant?: 'panel' | 'wash' | 'deep';
  pad?: number | string;
  glow?: 'vale' | 'surf';
  hover?: boolean;
  radius?: number;
  children?: ReactNode;
  style?: CSSProperties;
};

export function Card({ variant = 'panel', pad = 24, glow, hover, radius, style, children, className, ...rest }: CardProps) {
  const cls = ['t-card', variant === 'wash' ? 't-card-wash' : variant === 'deep' ? 't-card-deep' : '', hover ? 't-card-hover' : '', className]
    .filter(Boolean)
    .join(' ');
  const sh = glow === 'vale' ? 'var(--shadow-card),var(--glow-tide)' : glow === 'surf' ? 'var(--shadow-card),var(--glow-surf)' : undefined;
  return (
    <div className={cls} style={{ padding: pad, borderRadius: radius, boxShadow: sh, ...style }} {...rest}>
      {children}
    </div>
  );
}

/** The glassy starred panel used across landing and app screens (kit: Bento / ABento). */
export function Bento({ pad = 0, style, children, className, variant, ...rest }: Omit<CardProps, 'variant'> & { variant?: 'landing' | 'app' }) {
  return (
    <div className={['bento', variant === 'app' ? 'bento-app' : '', className].filter(Boolean).join(' ')} style={style} {...rest}>
      <div className="stars" aria-hidden />
      <div className="bento-in" style={{ padding: pad }}>{children}</div>
    </div>
  );
}
