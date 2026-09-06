import { useEffect, useRef, useState, type CSSProperties, type ReactNode } from 'react';
import { Link } from 'react-router';
import { ArrowRight, ChevronRight, Minus, Plus, Shield, Star, X, Zap } from 'lucide-react';

/* ---------- layout ---------- */
export function Container({ children, max = 1260, style, className }: { children: ReactNode; max?: number; style?: CSSProperties; className?: string }) {
  return <div className={['fx-container', className].filter(Boolean).join(' ')} style={{ maxWidth: max, ...style }}>{children}</div>;
}

export function Section({ id, children, className, style }: { id?: string; children: ReactNode; className?: string; style?: CSSProperties }) {
  return <section id={id} className={['fx-sec', className].filter(Boolean).join(' ')} style={style}>{children}</section>;
}

/** The scenery band that sits behind the bottom of many sections, faded in from white. */
export function BgItem({ src, top, bottom, overlay, height = 282, style }: { src: string; top?: boolean; bottom?: boolean; overlay?: number; height?: number; style?: CSSProperties }) {
  return (
    <div className="fx-bgitem" aria-hidden style={{ height, ...style }}>
      {top && <div className="fx-bg-top" />}
      {bottom && <div className="fx-bg-bottom" />}
      {overlay !== undefined && <div className="fx-bg-overlay" style={{ opacity: overlay }} />}
      <img src={src} alt="" className="fx-bgimg" />
    </div>
  );
}

/* ---------- text ---------- */
export function PreTitle({ children, variant = 'default' }: { children: ReactNode; variant?: 'default' | 'white' | 'dark' }) {
  return <span className={`fx-pretitle fx-pretitle-${variant}`}>{children}</span>;
}

export function Badge({ children, variant = 'default' }: { children: ReactNode; variant?: 'default' | 'white' }) {
  return <span className={`fx-badge fx-badge-${variant}`}>{children}</span>;
}

type ListIcon = 'check' | 'star' | 'shield' | 'zap' | 'x' | 'chevron';
function Glyph({ icon, size }: { icon: ListIcon; size: number }) {
  switch (icon) {
    case 'star': return <Star size={size} strokeWidth={0} fill="#FDBB6E" />;
    case 'shield': return <Shield size={size} strokeWidth={0} fill="#10B981" />;
    case 'zap': return <Zap size={size} strokeWidth={0} fill="#F51C23" />;
    case 'x': return <X size={size} strokeWidth={2.5} color="#F51C23" />;
    default: return <ChevronRight size={size} strokeWidth={2.5} color="#10B981" />;
  }
}

export function ListItem({ children, icon = 'chevron', variant = 'default', color }: { children: ReactNode; icon?: ListIcon; variant?: 'default' | 'fit' | 'lg'; color?: string }) {
  const sz = variant === 'fit' ? 18 : variant === 'lg' ? 14 : 12;
  return (
    <span className={`fx-li fx-li-${variant}`} style={color ? { color } : undefined}>
      <span className="fx-li-ic"><Glyph icon={icon} size={sz} /></span>
      <span>{children}</span>
    </span>
  );
}

/* ---------- buttons ---------- */
type LinkProps = { to?: string; href?: string; newTab?: boolean; onClick?: () => void };
function Clickable({ to, href, newTab, onClick, className, children, ariaLabel }: LinkProps & { className: string; children: ReactNode; ariaLabel?: string }) {
  if (to) return <Link to={to} className={className} onClick={onClick} aria-label={ariaLabel}>{children}</Link>;
  if (href) return <a href={href} className={className} onClick={onClick} target={newTab ? '_blank' : undefined} rel={newTab ? 'noreferrer' : undefined} aria-label={ariaLabel}>{children}</a>;
  return <button type="button" className={className} onClick={onClick} aria-label={ariaLabel}>{children}</button>;
}

/** Framer "Button Icon": halo ring + gradient pill + white arrow disc. */
export function BtnIcon({ children, variant = 'default', ...link }: LinkProps & { children: ReactNode; variant?: 'default' | 'dark' | 'nav' }) {
  return (
    <Clickable {...link} className={`fx-btnic fx-btnic-${variant}`}>
      <span className="fx-btnic-in">
        <span>{children}</span>
        <span className="fx-btnic-ic" aria-hidden><ArrowRight size={12} strokeWidth={2.5} /></span>
      </span>
    </Clickable>
  );
}

/** Framer "Button": plain pill. */
export function Btn({ children, variant = 'default', block, ...link }: LinkProps & { children: ReactNode; variant?: 'default' | 'dark' | 'mist'; block?: boolean }) {
  return <Clickable {...link} className={`fx-btn fx-btn-${variant}${block ? ' fx-btn-block' : ''}`}>{children}</Clickable>;
}

/* ---------- cards ---------- */
export function StatItem({ title, description, size = 'default', titleColor, descriptionColor }: { title: ReactNode; description: ReactNode; size?: 'default' | 'sm'; titleColor?: string; descriptionColor?: string }) {
  return (
    <div className={`fx-stat fx-stat-${size}`}>
      <div className="fx-stat-t" style={titleColor ? { color: titleColor } : undefined}>{title}</div>
      <div className="fx-stat-d" style={descriptionColor ? { color: descriptionColor } : undefined}>{description}</div>
    </div>
  );
}

export function StatLGCard({ preTitle, title, description, icon, variant = 'default', style, className }: { preTitle: ReactNode; title: ReactNode; description: ReactNode; icon: ReactNode; variant?: 'default' | 'dark' | 'primary' | 'white'; style?: CSSProperties; className?: string }) {
  return (
    <div className={['fx-statlg', `fx-statlg-${variant}`, className].filter(Boolean).join(' ')} style={style}>
      <div className="fx-statlg-top">
        <span className="fx-statlg-pre">{preTitle}</span>
        <span className="fx-statlg-ic">{icon}</span>
      </div>
      <div>
        <div className="fx-statlg-t">{title}</div>
        <div className="fx-statlg-d">{description}</div>
      </div>
    </div>
  );
}

export function StepCard({ number, title, description, variant = 'default' }: { number: string; title: ReactNode; description: ReactNode; variant?: 'default' | 'dark' | 'primary' }) {
  return (
    <div className={`fx-stepcard fx-stepcard-${variant}`}>
      <span className="fx-stepcard-n">{number}</span>
      <div>
        <div className="fx-stepcard-t">{title}</div>
        <div className="fx-stepcard-d">{description}</div>
      </div>
    </div>
  );
}

export function CapabilityCard({ icon, title, description }: { icon: ReactNode; title: ReactNode; description: ReactNode }) {
  return (
    <div className="fx-cap">
      <span className="fx-cap-ic">{icon}</span>
      <div>
        <div className="fx-cap-t">{title}</div>
        <div className="fx-cap-d">{description}</div>
      </div>
    </div>
  );
}

export function IntegrationCard({ icon, title, description }: { icon: ReactNode; title: ReactNode; description: ReactNode }) {
  return (
    <div className="fx-intcard">
      <span className="fx-intcard-ic">{icon}</span>
      <div>
        <div className="fx-intcard-t">{title}</div>
        <div className="fx-intcard-d">{description}</div>
      </div>
    </div>
  );
}

export function OverviewCard({ icon, lead, children }: { icon: ReactNode; lead: ReactNode; children: ReactNode }) {
  return (
    <div className="fx-ovcard">
      <span className="fx-ovcard-ic">{icon}</span>
      <p className="fx-ovcard-p"><span>{lead}</span> {children}</p>
    </div>
  );
}

export function TestimonialItem({ content, name, jobTitle, avatar }: { content: ReactNode; name: ReactNode; jobTitle: ReactNode; avatar: string }) {
  return (
    <div className="fx-testi">
      <div>
        <div className="fx-testi-stars" aria-label="5 out of 5 stars">{[0, 1, 2, 3, 4].map((i) => <Star key={i} size={18} strokeWidth={0} fill="#FDBB6E" />)}</div>
        <p className="fx-testi-p">{content}</p>
      </div>
      <div className="fx-testi-b">
        <img src={avatar} alt="" className="fx-testi-av" width={50} height={50} />
        <div>
          <div className="fx-testi-n">{name}</div>
          <div className="fx-testi-j">{jobTitle}</div>
        </div>
      </div>
    </div>
  );
}

export function UseCaseCard({ title, description, statsTitle, statsDescription, bg }: { title: ReactNode; description: ReactNode; statsTitle: ReactNode; statsDescription: ReactNode; bg: string }) {
  return (
    <div className="fx-usecase" style={{ backgroundImage: `url(${bg})` }}>
      <div className="fx-usecase-in">
        <div>
          <div className="fx-usecase-t">{title}</div>
          <p className="fx-usecase-d">{description}</p>
        </div>
        <div>
          <div className="fx-usecase-st">{statsTitle}</div>
          <div className="fx-usecase-sd">{statsDescription}</div>
        </div>
      </div>
    </div>
  );
}

export function FounderCard({ content, avatar, jobTitle }: { content: ReactNode; avatar: string; jobTitle: ReactNode }) {
  return (
    <div className="fx-founder">
      <p className="fx-founder-q">{content}</p>
      <div className="fx-founder-b"><img src={avatar} alt="" width={30} height={30} /><span>{jobTitle}</span></div>
    </div>
  );
}

export function Accordion({ question, answer, open: initial }: { question: ReactNode; answer: ReactNode; open?: boolean }) {
  const [open, setOpen] = useState(!!initial);
  return (
    <div className="fx-acc" data-open={open}>
      <button type="button" className="fx-acc-q" onClick={() => setOpen((o) => !o)} aria-expanded={open}>
        <span>{question}</span>
        <span className="fx-acc-ic" aria-hidden>{open ? <Minus size={16} strokeWidth={2.5} /> : <Plus size={16} strokeWidth={2.5} />}</span>
      </button>
      <div className="fx-acc-a" hidden={!open}><p>{answer}</p></div>
    </div>
  );
}

export function FaqsCta({ avatars }: { avatars: string[] }) {
  return (
    <div className="fx-faqcta">
      <div className="fx-faqcta-top">
        <span className="fx-faqcta-avs">{avatars.map((a) => <img key={a} src={a} alt="" width={40} height={40} />)}</span>
        <span>+</span>
        <span className="fx-faqcta-you">You</span>
      </div>
      <div>
        <div className="fx-faqcta-t">Still have questions?</div>
        <div className="fx-faqcta-d">Reach out, and our team will guide you.</div>
      </div>
      <BtnIcon variant="dark" href="mailto:hello@valeprotocol.xyz">Talk to our team</BtnIcon>
    </div>
  );
}

/** Dashboard screenshot with a phone-layout capture served below the phone breakpoint. */
export function Shot({ name, alt, width, height, mobileHeight }: { name: string; alt: string; width: number; height: number; mobileHeight: number }) {
  return (
    <picture>
      <source media="(max-width: 809px)" srcSet={`/fx/${name}-m.jpg`} width={780} height={mobileHeight * 2} />
      <img src={`/fx/${name}.jpg`} alt={alt} width={width} height={height} loading="lazy" decoding="async" />
    </picture>
  );
}

/* ---------- motion ---------- */
/** Infinite horizontal marquee: duplicates its children and scrolls the track. */
export function Ticker({ children, gap = 70, duration = 40, align = 'center', reverse }: { children: ReactNode; gap?: number; duration?: number; align?: 'center' | 'end'; reverse?: boolean }) {
  return (
    <div className="fx-ticker" style={{ '--gap': `${gap}px`, '--dur': `${duration}s`, alignItems: align === 'end' ? 'flex-end' : 'center' } as CSSProperties} data-reverse={reverse || undefined}>
      <div className="fx-ticker-track">{children}</div>
      <div className="fx-ticker-track" aria-hidden>{children}</div>
    </div>
  );
}

/** Rotating tab strip with a 7s progress bar under the active tab (Framer "Step Switcher" + ProgressAnimation). */
export function StepSwitcher({ labels, active, onChange, interval = 7000 }: { labels: string[]; active: number; onChange: (i: number) => void; interval?: number }) {
  const ref = useRef(onChange);
  useEffect(() => { ref.current = onChange; }, [onChange]);
  useEffect(() => {
    const t = setInterval(() => ref.current((active + 1) % labels.length), interval);
    return () => clearInterval(t);
  }, [active, labels.length, interval]);
  return (
    <div className="fx-steps-tabs" role="tablist">
      <span className="fx-steps-line" aria-hidden />
      <span className="fx-steps-lineL" aria-hidden />
      <span className="fx-steps-lineR" aria-hidden />
      {labels.map((l, i) => (
        <button key={l} type="button" role="tab" aria-selected={i === active} className="fx-stepsw" data-on={i === active} onClick={() => onChange(i)} style={{ '--t': `${interval}ms` } as CSSProperties}>
          {l}
          {i === active && <span className="fx-stepsw-prog" key={`${i}-${active}`} />}
        </button>
      ))}
    </div>
  );
}
