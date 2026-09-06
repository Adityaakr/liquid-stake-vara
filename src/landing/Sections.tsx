import { useEffect, useRef, useState, type ReactNode } from 'react';
import { ArrowLeftRight, Bell, Boxes, Coins, Globe, Layers, Lightbulb, Link2, Lock, Repeat, Rocket, ShieldCheck, Sparkles, Timer, Users, Wallet, Zap } from 'lucide-react';
import { TokenIcon } from '@/ui';
import { Badge, BgItem, Btn, BtnIcon, Container, FounderCard, ListItem, OverviewCard, PreTitle, Section, Shot, StatItem, StatLGCard, StepSwitcher, TestimonialItem, Ticker, UseCaseCard } from './fx';

const FX = '/fx/';

/* ---------- Comparison: before / after ---------- */
export function Comparison() {
  const [side, setSide] = useState<'before' | 'after'>('before');
  const spacer = useRef<HTMLDivElement>(null);
  useEffect(() => {
    const el = spacer.current;
    if (!el || typeof IntersectionObserver === 'undefined') return;
    const io = new IntersectionObserver(([e]) => { if (e.isIntersecting) setSide('after'); }, { threshold: 0.6 });
    io.observe(el);
    return () => io.disconnect();
  }, []);
  const before = ['Staked VARA is locked for a full 7-day unbonding period', 'No way to put staked capital to work in DeFi', 'Rewards need manual claiming and re-staking', 'Below 50 VARA you cannot nominate at all'];
  const after = ['Get kVARA instantly and exit through the buffer or DEX', 'Post kVARA as collateral, borrow stables, loop your stake', 'Rewards compound into the rate every era, no claiming ever', 'Any amount is pooled and nominated together'];
  return (
    <Section id="products" className="fx-compare">
      <Container max={760} className="fx-compare-wrap">
        <div className="fx-compare-in">
          <h2 className="fx-h2">Smarter staking starts with liquidity</h2>
          <div className="fx-ba" data-side={side}>
            <div className="fx-ba-tabs" role="tablist">
              <div className="fx-ba-overlay-top" aria-hidden />
              <button type="button" role="tab" className="fx-ba-tab" data-side="before" aria-selected={side === 'before'} onClick={() => setSide('before')}>Before Vale Protocol</button>
              <span className="fx-ba-knob" aria-hidden><img src={`${FX}BepIwACX380EUEqBMK5sdcQgh3k.png`} alt="" width={130} /></span>
              <button type="button" role="tab" className="fx-ba-tab" data-side="after" aria-selected={side === 'after'} onClick={() => setSide('after')}>After Vale Protocol</button>
            </div>
            <div className="fx-ba-frame">
              <div className="fx-ba-panel">
                <div className="fx-ba-desc">
                  <h3>{side === 'before' ? 'Challenges of staking VARA today' : 'Smarter way to stake your VARA'}</h3>
                  <div className="fx-ba-list">
                    {(side === 'before' ? before : after).map((t) => <ListItem key={t} variant="lg" icon={side === 'before' ? 'x' : 'chevron'} color={side === 'after' ? '#BABABA' : undefined}>{t}</ListItem>)}
                  </div>
                </div>
                <div className="fx-ba-stats">
                  {(side === 'before' ? [['68%', 'Capital sitting idle'], ['55%', 'Missed compounding']] : [['3X Faster', 'Capital efficiency'], ['24/7', 'Exits at the live rate']]).map(([b, s]) => (
                    <div key={b} className="fx-ba-statcard"><b>{b}</b><span>{s}</span></div>
                  ))}
                </div>
                <div className="fx-ba-bg" aria-hidden><img src={`${FX}Osh2UHQcarC8mZnL6oh6gwYnbA.jpg`} alt="" /></div>
              </div>
            </div>
          </div>
        </div>
      </Container>
      <div ref={spacer} className="fx-compare-spacer" aria-hidden />
    </Section>
  );
}

/* ---------- Features bento ---------- */
const WORDS: [string, string][] = [['Swap', '#F28778'], ['Exit', '#8AE389'], ['Loop', '#FDBB6E']];
function FeatureBoxTitle() {
  const [i, setI] = useState(0);
  useEffect(() => { const t = setInterval(() => setI((x) => (x + 1) % WORDS.length), 2600); return () => clearInterval(t); }, []);
  return (
    <div className="fx-fbt" aria-hidden>
      {WORDS.map(([w, c], k) => (
        <div key={w} className="fx-fbt-word" data-on={k === i} style={{ color: c }}>{w}<i>{w}</i></div>
      ))}
    </div>
  );
}

export function Features() {
  return (
    <Section id="features" className="fx-features">
      <Container>
        <div className="fx-features-head">
          <div>
            <PreTitle>Core features</PreTitle>
            <h2 className="fx-h2">Everything you need to stake confidently</h2>
          </div>
          <div>
            <p className="fx-body">Protocol-grade tooling designed for solo stakers and treasuries managing large VARA positions.</p>
            <BtnIcon variant="dark" to="/features">View all features</BtnIcon>
          </div>
        </div>
        <div className="fx-features-grid">
          <div className="fx-features-left">
            <div className="fx-fcard">
              <h3>Conservative risk parameters</h3>
              <img src={`${FX}wXWMCe97v6E9fhERLabXGgt0Go.png`} alt="" style={{ maxWidth: 150, position: 'relative', zIndex: 1 }} />
              <div className="fx-fcard-badges">
                <Badge variant="white">50% max LTV</Badge>
                <Badge variant="white">Insurance fund first</Badge>
                <Badge variant="white">7-day slash defer</Badge>
              </div>
            </div>
            <div className="fx-fcard">
              <h3>A rate that only rises</h3>
              <img src={`${FX}G7W3uXGEWJ2M7xixcWnVJPdzxEc.svg`} alt="" style={{ width: '100%', position: 'relative', zIndex: 1 }} />
              <BgItem src={`${FX}IZgCL46gW5tJW2TUtUKnT2MFgMs.jpg`} overlay={0.8} />
            </div>
            <div className="fx-fcard fx-fcard-3">
              <div className="fx-fcard-top">
                <h3>Portfolio in one view</h3>
                <p>See every position at once, with the live redemption rate, era rewards and vault share prices in one place.</p>
              </div>
              <div className="fx-fcard-shots">
                <img src={`${FX}LLeScfWulZWyA0jZhMBwmQgQHA.svg`} alt="" style={{ width: 250 }} />
                <img src={`${FX}LP5Ny6NdFTClX39IhTGUIolDygE.svg`} alt="" style={{ width: 300 }} />
              </div>
              <BgItem src={`${FX}subirXJz7lXrSNejZPxoXXA90Ik.jpg`} overlay={0.9} />
            </div>
          </div>
          <div className="fx-features-right">
            <div className="fx-fcard fx-fcard-dark">
              <h3>Every exit path</h3>
              <FeatureBoxTitle />
              <p>Swap instantly, unbond natively, or loop into stables.</p>
            </div>
            <div className="fx-fcard" style={{ minHeight: 0 }}>
              <h3>Smart alerts</h3>
              <div className="fx-alert">
                <img src={`${FX}e7N84L0vlZQpYYqGdndB2EPeKk.svg`} alt="" style={{ width: '100%', top: 0, zIndex: 3 }} />
                <img src={`${FX}qRR37bNghXttLizbZI0gYViJtDE.svg`} alt="" style={{ width: '86%', top: 44, zIndex: 2 }} />
                <img src={`${FX}KeTabRzcIFVfk1ymsNMeghVPnM.svg`} alt="" style={{ width: '72%', top: 76, zIndex: 1 }} />
                <img src={`${FX}dIhxLbN1GBZu7TFHjssn0JUI4kY.png`} alt="" className="fx-alert-bell" />
              </div>
            </div>
          </div>
        </div>
      </Container>
    </Section>
  );
}

/* ---------- Platform overview ---------- */
export function Overview() {
  return (
    <Section className="fx-overview">
      <Container>
        <div className="fx-overview-content">
          <div className="fx-overview-top">
            <div className="fx-top">
              <PreTitle>Platform overview</PreTitle>
              <h2 className="fx-h2 fx-center">See your staking in action</h2>
              <p className="fx-body fx-center">Explore a live dashboard that brings your stake, vault positions and unbonding queue together in one clear view.</p>
            </div>
            <div className="fx-buttons">
              <BtnIcon to="/features">Explore features</BtnIcon>
              <Btn variant="dark" to="/app">Try the live demo</Btn>
            </div>
          </div>
          <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 30, width: '100%' }}>
            <div className="fx-overview-shot"><div><Shot name="app-overview" alt="The Vale Protocol vaults dashboard" width={1400} height={846} mobileHeight={760} /></div></div>
            <div className="fx-overview-cards">
              <OverviewCard icon={<Rocket size={20} strokeWidth={2} />} lead="All your positions in one place:">Stake, vault shares and unbonding queue together in one clear, unified view.</OverviewCard>
              <OverviewCard icon={<Zap size={20} strokeWidth={2} />} lead="Make progress faster:">One transaction to stake, one to exit, with no claiming and no waiting on rewards.</OverviewCard>
              <OverviewCard icon={<Lightbulb size={20} strokeWidth={2} />} lead="Built for better focus:">A clean interface that keeps the rate front and centre and everything else simple.</OverviewCard>
            </div>
          </div>
        </div>
      </Container>
      <BgItem src={`${FX}sGvx8VOXGYVGBocxGp5Wy6GfeA.jpg`} top bottom />
    </Section>
  );
}

/* ---------- How it works ---------- */
const STEPS: [string, string, string][] = [
  ['ykcQXfkRR4KTqMLXTbg0T9PB8zk.png', 'Connect your wallet', 'Link any Substrate wallet in one click. Your keys never leave it.'],
  ['BRYQbNFP21XSdbOpg2tEs4MO0aY.png', 'Stake and mint kVARA', 'Deposit VARA and receive kVARA at the live rate in a single transaction.'],
  ['nDs3OgbLpR77rcn1GvmpOSivzdI.png', 'Earn every era', 'Rewards compound into the rate automatically. Exit instantly or unbond natively.'],
];
export function Steps() {
  const [i, setI] = useState(0);
  return (
    <Section id="how-it-works" className="fx-steps">
      <Container>
        <div className="fx-steps-content">
          <div className="fx-steps-left">
            <div className="fx-top-left">
              <PreTitle>How it works</PreTitle>
              <h2 className="fx-h2">Start staking in minutes</h2>
              <p className="fx-body">Connect a wallet, stake VARA, and let the redemption rate do the rest while you stay liquid.</p>
            </div>
            <div className="fx-steps-stats">
              <StatItem title="100%" description="Non-custodial, on-chain protocol" />
              <StatItem title="2 Minutes" description="From connect to first stake" />
            </div>
          </div>
          <div className="fx-workflow">
            <StepSwitcher labels={['Step 01', 'Step 02', 'Step 03']} active={i} onChange={setI} />
            <div className="fx-workflow-frame">
              <div className="fx-workflow-panel">
                {STEPS.map(([img, t, d], k) => (
                  <div key={t} className="fx-workflow-step" data-on={k === i}>
                    <img src={FX + img} alt="" />
                    <div className="fx-workflow-desc"><h4>{t}</h4><p className="fx-body">{d}</p></div>
                  </div>
                ))}
              </div>
            </div>
          </div>
        </div>
      </Container>
    </Section>
  );
}

/* ---------- Security ---------- */
export function Security() {
  return (
    <Section id="security">
      <Container>
        <div className="fx-split">
          <div className="fx-imgpanel fx-imgpanel-wide"><img src={`${FX}ShWpChwHKJXjkAo8ZPOmvydAuFE.png`} alt="" /></div>
          <div className="fx-split-col fx-split-narrow">
            <div className="fx-top-left">
              <PreTitle>Security &amp; compliance</PreTitle>
              <h2 className="fx-h2">Your stake is protected at every level</h2>
              <div style={{ marginTop: 10 }}><BtnIcon variant="dark" to="/app">Get started now</BtnIcon></div>
            </div>
            <div className="fx-split-list">
              <ListItem>Insurance fund fills before the treasury</ListItem>
              <ListItem>Curated, screened validator set</ListItem>
              <ListItem>Slashes deferred 7 days for governance</ListItem>
              <ListItem>Independent audits and a public bug bounty</ListItem>
            </div>
          </div>
        </div>
      </Container>
    </Section>
  );
}

/* ---------- Use cases ---------- */
const CASES: [string, string, string, string, string][] = [
  ['Individual stakers', 'Stake any amount, hold kVARA, and let the rate compound without claiming or guesswork.', '+32% Faster', 'Capital back to work with instant exits', 'l4todGvJ7jL3WhxngE8F0mr2mks.jpg'],
  ['DAOs and treasuries', 'Keep the treasury staked and liquid at the same time, with positions visible to everyone.', 'Real-time', 'Reporting across every position', 'rF4nkbqTtccZBPQrR1TG1yKqD8.jpg'],
  ['Validators', 'Grow a curated nomination pool with transparent commission, uptime and slash history.', '10+', 'Validators nominated from one pool', 'Rl52kJV49NxXXh2BR2NJhjJuN7I.jpg'],
  ['Loopers', 'Post kVARA as collateral, borrow stables, and amplify staking yield with conservative LTVs.', 'Up to 2X', 'Effective stake on the same VARA', 'nPSRrYomcOALmLPJavAfjBIboI.jpg'],
];
const PHOTOS = ['QJxW9eYj0OFH6UvNuY4WNkVNJ0.jpg', 'ir92iMEO3JaF66SNrwrpEtYCY.jpg', 'nkS8pAWPKcD8U7ncGVFOqXVV0o.jpg', 'omPOmfMdMLLw0U7G41FgZY7Cfmg.jpg'];
export function UseCases() {
  return (
    <Section id="use-cases" className="fx-usecases">
      <div className="fx-top">
        <PreTitle>Use cases</PreTitle>
        <h2 className="fx-h2 fx-center">Who this protocol is built for</h2>
      </div>
      <div className="fx-usecases-ticker">
        <Ticker gap={10} duration={60}>
          {CASES.map(([t, d, st, sd, bg], k) => [
            <img key={`p${k}`} src={FX + PHOTOS[k]} alt="" className="fx-usecase-photo" />,
            <UseCaseCard key={t} title={t} description={d} statsTitle={st} statsDescription={sd} bg={FX + bg} />,
          ])}
        </Ticker>
      </div>
      <div className="fx-usecases-bottom">
        <div className="fx-badges">
          <Badge>12,400+ Holders</Badge><Badge>4.9 Rating</Badge><Badge>Real-time rate</Badge><Badge>Audited &amp; insured</Badge>
        </div>
        <FounderCard content="“We built Vale Protocol to remove the lock-up from staking and give people a receipt token that composes with everything.”" avatar={`${FX}7Z2d6WeDiCpoz0B6ookMTPOFAU.jpg`} jobTitle="Founder &amp; CEO" />
      </div>
    </Section>
  );
}

/* ---------- Integrations ---------- */
const RING: ReactNode[] = [
  <TokenIcon token="VARA" size={32} />, <Wallet size={30} color="#406AE4" />, <TokenIcon token="kUSDT" size={32} />, <ArrowLeftRight size={30} color="#10B981" />,
  <TokenIcon token="kUSDC" size={32} />, <Boxes size={30} color="#FF8B06" />, <Link2 size={30} color="#5290F4" />, <Repeat size={30} color="#F28778" />,
  <TokenIcon token="kVARA" size={32} />, <Globe size={30} color="#3B82F6" />, <TokenIcon token="USDT" size={32} />, <Layers size={30} color="#1D1D1D" />,
  <TokenIcon token="USDC" size={32} />, <Coins size={30} color="#FDBB6E" />, <Lock size={30} color="#F51C23" />, <Sparkles size={30} color="#8AE389" />,
];
export function Integrations() {
  return (
    <Section id="integrations">
      <Container>
        <div className="fx-integrations-card">
          <div className="fx-integrations-top">
            <div className="fx-top">
              <PreTitle variant="white">Integrations</PreTitle>
              <h2 className="fx-h2 fx-center">Connect with the tools you already use</h2>
              <p className="fx-body fx-center">kVARA is a plain, non-rebasing token, so it drops into every wallet, DEX, bridge and lending market on Vara.</p>
            </div>
            <BtnIcon href="https://wiki.vara.network/" newTab>Explore the ecosystem</BtnIcon>
          </div>
          <div className="fx-ring-wrap">
            <div className="fx-ring" aria-hidden>
              {RING.map((ic, k) => <span key={k} className="fx-ring-item" style={{ '--a': `${k * 22.5}deg` } as React.CSSProperties}><span>{ic}</span></span>)}
            </div>
            <div className="fx-ring-center">
              <span className="fx-ring-core"><img src={`${FX}4LS9gC9h4W4WbsmhQmoxQ7DsncQ.svg`} alt="" style={{ height: 40 }} /></span>
              <h3 className="fx-h6">Composable with the whole Vara ecosystem and growing</h3>
            </div>
          </div>
          <img src={`${FX}XlxsE037ei7LGhxYPQDC3jctO9A.png`} alt="" className="fx-integrations-bg" aria-hidden />
        </div>
      </Container>
    </Section>
  );
}

/* ---------- Stats ---------- */
const STATS: { pre: string; title: string; desc: string; icon: ReactNode; variant: 'default' | 'dark' | 'primary'; pos: React.CSSProperties }[] = [
  { pre: 'Active stakers', title: '12,400+', desc: 'Wallets, pools and vaults holding kVARA.', icon: <Users size={20} />, variant: 'default', pos: { left: 50, top: 130 } },
  { pre: 'Total value locked', title: '$1.2M+', desc: 'Across the kVARA, kUSDT and kUSDC pools.', icon: <Coins size={20} />, variant: 'dark', pos: { right: 0, top: 120 } },
  { pre: 'Rewards paid', title: '940K', desc: 'VARA compounded into the rate, era by era.', icon: <Sparkles size={20} />, variant: 'dark', pos: { left: 20, top: 620 } },
  { pre: 'Validators covered', title: '120+', desc: 'Screened for commission, uptime and history.', icon: <Globe size={20} />, variant: 'primary', pos: { left: 430, top: 680 } },
  { pre: 'Protocol uptime', title: '99.9%', desc: 'Reliable access to your stake and exits.', icon: <Timer size={20} />, variant: 'default', pos: { right: 0, top: 590 } },
];
export function Stats() {
  return (
    <Section className="fx-stats">
      <Container>
        <div className="fx-stats-board">
          <div className="fx-top">
            <PreTitle>Protocol stats</PreTitle>
            <h2 className="fx-h2 fx-center">Powering smarter staking decisions</h2>
            <p className="fx-body fx-center">Real-time rates, deep exits and a conservative risk engine working together.</p>
          </div>
          <div className="fx-stats-grid" style={{ display: 'contents' }}>
            {STATS.map((s) => <StatLGCard key={s.pre} preTitle={s.pre} title={s.title} description={s.desc} icon={s.icon} variant={s.variant} style={s.pos} />)}
          </div>
        </div>
      </Container>
    </Section>
  );
}

/* ---------- Testimonials ---------- */
const TESTI: [string, string, string, string][] = [
  ['Vale Protocol let me keep staking while I actually use my VARA. The rate is transparent and the exits just work.', '7Z2d6WeDiCpoz0B6ookMTPOFAU.jpg', 'David Miller', 'Individual staker'],
  ['Managing a treasury position is far easier now. kVARA sits in our vault and the reporting saves us hours every week.', 'hYfCvJ3IVdEznEOwIQiiAxWOsPY.jpg', 'Sarah Thompson', 'DAO treasurer'],
  ['Instant exits through the buffer mean I can react to the market the moment it moves. It is part of my daily workflow.', '622M5cyJBdKPIK1fPnBlo3qONk.jpg', 'Michael Chen', 'Active trader'],
  ['The vault dashboard makes the share price and utilization far easier to interpret than any other Vara protocol.', 'W13V3WO2YwDah4yBxCcZc70Es.jpg', 'Emily Rodriguez', 'DeFi analyst'],
  ['Clear parameters, an insurance fund that fills first, and a rate that only rises. That is exactly what I want to nominate into.', '5O8P63EQwkFO1m5OTR4jsw7hI8.jpg', 'Daniel Carter', 'Validator operator'],
];
export function Testimonials() {
  return (
    <Section className="fx-testimonials">
      <Container>
        <div className="fx-testimonials-head">
          <div>
            <h2 className="fx-h2">What stakers say about the protocol</h2>
            <div className="fx-hero-list" style={{ justifyContent: 'flex-start' }}>
              <ListItem variant="fit" icon="star">4.9/5 Rating</ListItem><span className="fx-line" />
              <ListItem variant="fit" icon="shield">75+ Testimonials</ListItem><span className="fx-line" />
              <ListItem variant="fit" icon="zap">12K+ Community</ListItem>
            </div>
          </div>
          <div><BtnIcon variant="dark" to="/app">Get started today</BtnIcon></div>
        </div>
      </Container>
      <div className="fx-testimonials-ticker">
        <Ticker gap={50} duration={70} align="end">
          {TESTI.map(([c, av, n, j]) => <TestimonialItem key={n} content={c} avatar={FX + av} name={n} jobTitle={j} />)}
        </Ticker>
      </div>
      <BgItem src={`${FX}cQBpXWVe2IileA0HCzW5W4uvgY.jpg`} top bottom height={420} />
    </Section>
  );
}

export { ShieldCheck, Bell };
