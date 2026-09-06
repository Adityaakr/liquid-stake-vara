import { useState } from 'react';
import { Link, NavLink } from 'react-router';
import { BtnIcon } from './fx';

/** The wave mark: the same drawing as the favicon, on the brand's deep violet tile. */
export function WaveMark({ size = 32, light }: { size?: number; light?: boolean }) {
  const stroke = light ? '#211C30' : 'url(#vale-wave)';
  return (
    <svg width={size} height={size} viewBox="0 0 64 64" aria-hidden style={{ display: 'block', flexShrink: 0 }}>
      <rect width="64" height="64" rx="16" fill={light ? '#FFFFFF' : '#211C30'} />
      <defs><linearGradient id="vale-wave" x1="0" y1="0" x2="1" y2="1"><stop offset="0" stopColor="#FFFFFF" /><stop offset="1" stopColor="#A3A4FF" /></linearGradient></defs>
      <path d="M14 40c6 0 6-8 12-8s6 8 12 8 6-8 12-8" fill="none" stroke={stroke} strokeWidth="5" strokeLinecap="round" />
      <path d="M14 28c6 0 6-8 12-8s6 8 12 8 6-8 12-8" fill="none" stroke={stroke} strokeWidth="5" strokeLinecap="round" opacity=".55" />
    </svg>
  );
}

export function Wordmark({ size = 24, href = '/', light }: { size?: number; href?: string; light?: boolean }) {
  return (
    <Link to={href} aria-label="Vale Protocol home" className="fx-wordmark" style={{ fontSize: size, color: light ? '#fff' : undefined }}>
      <WaveMark size={Math.round(size * 1.33)} light={light} />
      vale
    </Link>
  );
}

const LINKS = [['/#products', 'Products'], ['/#features', 'Features'], ['/#use-cases', 'Use Cases'], ['/features', 'Product']] as const;

export function LandingNav() {
  const [open, setOpen] = useState(false);
  return (
    <div className="fx-nav-wrap">
      <div className="fx-nav">
        <div className="fx-nav-in">
          <div className="fx-nav-left"><Wordmark /></div>
          <nav className="fx-nav-menu" aria-label="Primary">
            {LINKS.map(([href, label]) => href.startsWith('/#')
              ? <a key={href} className="fx-navlink" href={href}>{label}</a>
              : <NavLink key={href} className="fx-navlink" to={href}>{label}</NavLink>)}
          </nav>
          <div className="fx-nav-right">
            <BtnIcon variant="nav" to="/app">Launch app</BtnIcon>
            <button type="button" className="fx-nav-burger" aria-label={open ? 'Close menu' : 'Open menu'} aria-expanded={open} onClick={() => setOpen((o) => !o)} />
          </div>
        </div>
        {open && (
          <div className="fx-nav-drop" role="menu">
            {LINKS.map(([href, label]) => href.startsWith('/#')
              ? <a key={href} className="fx-navlink" href={href} role="menuitem" onClick={() => setOpen(false)}>{label}</a>
              : <NavLink key={href} className="fx-navlink" to={href} role="menuitem" onClick={() => setOpen(false)}>{label}</NavLink>)}
          </div>
        )}
      </div>
    </div>
  );
}
