import { useState } from 'react';
import { Accordion, Btn, BtnIcon, Container, FaqsCta, ListItem, PreTitle, Section } from './fx';
import { Wordmark } from './Nav';

const FX = '/fx/';
const SOCIALS: [string, string, string][] = [
  ['https://www.instagram.com/', 'Instagram', 'M12 7.3a4.7 4.7 0 1 0 0 9.4 4.7 4.7 0 0 0 0-9.4Zm0 7.7a3 3 0 1 1 0-6 3 3 0 0 1 0 6Zm5-8a1.1 1.1 0 1 1-2.2 0 1.1 1.1 0 0 1 2.2 0ZM12 2c-2.7 0-3.1 0-4.1.1-2.9.1-4.7 1.9-4.8 4.8C3 7.9 3 8.3 3 12s0 4.1.1 5.1c.1 2.9 1.9 4.7 4.8 4.8 1 .1 1.4.1 4.1.1s3.1 0 4.1-.1c2.9-.1 4.7-1.9 4.8-4.8.1-1 .1-1.4.1-5.1s0-4.1-.1-5.1c-.1-2.9-1.9-4.7-4.8-4.8C15.1 2 14.7 2 12 2Zm0 1.7c2.7 0 3 0 4 .1 2.1.1 3.1 1.1 3.2 3.2.1 1 .1 1.3.1 4s0 3-.1 4c-.1 2.1-1.1 3.1-3.2 3.2-1 .1-1.3.1-4 .1s-3 0-4-.1c-2.1-.1-3.1-1.1-3.2-3.2-.1-1-.1-1.3-.1-4s0-3 .1-4c.1-2.1 1.1-3.1 3.2-3.2 1-.1 1.3-.1 4-.1Z'],
  ['https://www.linkedin.com/', 'LinkedIn', 'M20.4 2H3.6A1.6 1.6 0 0 0 2 3.6v16.8A1.6 1.6 0 0 0 3.6 22h16.8a1.6 1.6 0 0 0 1.6-1.6V3.6A1.6 1.6 0 0 0 20.4 2ZM8 19H5V9h3v10ZM6.5 7.7a1.7 1.7 0 1 1 0-3.5 1.7 1.7 0 0 1 0 3.5ZM19 19h-3v-4.9c0-1.2 0-2.7-1.6-2.7s-1.9 1.3-1.9 2.6V19h-3V9h2.9v1.4c.4-.8 1.4-1.6 2.9-1.6 3.1 0 3.7 2 3.7 4.7V19Z'],
  ['https://www.facebook.com', 'Facebook', 'M13.5 22v-8.2h2.8l.4-3.2h-3.2V8.5c0-.9.3-1.6 1.6-1.6h1.7V4.1c-.3 0-1.3-.1-2.5-.1-2.5 0-4.2 1.5-4.2 4.3v2.3H7.3v3.2h2.8V22h3.4Z'],
  ['https://x.com/', 'X', 'M17.8 3h3l-6.7 7.6L22 21h-6.2l-4.8-6.3L5.4 21h-3l7.1-8.2L2 3h6.3l4.4 5.8L17.8 3Zm-1.1 16.2h1.7L7.4 4.7H5.6l11.1 14.5Z'],
];

/* ---------- Pricing → fees ---------- */
export function Pricing() {
  const [instant, setInstant] = useState(true);
  const exitFee = instant ? '0.3' : '0';
  return (
    <Section id="pricing" className="fx-pricing">
      <Container>
        <div className="fx-pricing-in">
          <div className="fx-top">
            <PreTitle>Protocol fees</PreTitle>
            <h2 className="fx-h2 fx-center">Transparent fees without hidden costs</h2>
          </div>
          <div className="fx-pricing-item">
            <div className="fx-pricing-toggle">
              <span data-off={!instant}>Instant exit</span>
              <button type="button" role="switch" aria-checked={!instant} aria-label="Toggle exit path" className="fx-toggle" onClick={() => setInstant((v) => !v)}><i /></button>
              <span data-off={instant} style={{ display: 'inline-flex', alignItems: 'center', gap: 6 }}>Native unbond <span className="fx-offer">0% fee</span></span>
              <span className="fx-pricing-toggle-line" aria-hidden />
            </div>
            <div className="fx-pricing-frame">
              <div className="fx-pricing-grid">
                <div className="fx-plan">
                  <div className="fx-plan-head">
                    <div><div className="fx-plan-t">Stake VARA</div><div className="fx-plan-s">Best for individual stakers</div></div>
                  </div>
                  <div className="fx-plan-price"><b>{exitFee}%</b><span>/ exit</span></div>
                  <Btn variant="dark" to="/app" block>Start staking</Btn>
                  <div className="fx-plan-list">
                    <ListItem>Mint kVARA at the live rate</ListItem>
                    <ListItem>Rewards compound every era</ListItem>
                    <ListItem>{instant ? 'Instant exit via the buffer or DEX' : 'Native unbond in 7 days, full rate'}</ListItem>
                    <ListItem>10% of era rewards to the protocol</ListItem>
                    <ListItem>Non-rebasing, wallet-friendly token</ListItem>
                    <ListItem>No claiming, ever</ListItem>
                  </div>
                </div>
                <div className="fx-plan fx-plan-dark">
                  <div className="fx-plan-head">
                    <div><div className="fx-plan-t">Stable vaults</div><div className="fx-plan-s">Best for yield on stables</div></div>
                    <span className="fx-plan-popular">Popular</span>
                  </div>
                  <div className="fx-plan-price"><b>0%</b><span>/ deposit</span></div>
                  <Btn to="/app/vaults" block>Open a vault</Btn>
                  <div className="fx-plan-list">
                    <ListItem>Deposit wUSDT or wUSDC</ListItem>
                    <ListItem>Earn borrow interest from loopers</ListItem>
                    <ListItem>ERC-4626 style share tokens</ListItem>
                    <ListItem>kVARA is the only collateral</ListItem>
                    <ListItem>Insurance fund fills before the treasury</ListItem>
                    <ListItem>Withdraw any time below the kink</ListItem>
                  </div>
                </div>
              </div>
              <div className="fx-pricing-foot"><span>No minimum stake</span><i /><span>No lock-up on kVARA</span><i /><span>Exit anytime</span></div>
            </div>
            <div className="fx-enterprise">
              <div>
                <h3 className="fx-h6">Validators &amp; institutions</h3>
                <p className="fx-body">Need a custom nomination set or treasury integration? Talk with our team to design a setup for your needs.</p>
              </div>
              <div><Btn variant="dark" href="mailto:hello@valeprotocol.xyz">Contact us</Btn></div>
              <img src={`${FX}KbVFR1CeRk5msFRW70lzRZLV8I.png`} alt="" aria-hidden />
            </div>
          </div>
        </div>
      </Container>
    </Section>
  );
}

/* ---------- FAQs ---------- */
const QS: [string, string][] = [
  ['How secure is my staked VARA?', 'Vale Protocol is non-custodial: your VARA is bonded on chain to a curated validator set and kVARA is minted to your wallet. Slashes are deferred seven days and an insurance fund fills before the treasury sees a token.'],
  ['Can I unstake instantly?', 'Yes. Swap kVARA for VARA from the liquidity buffer or the DEX for a 0.3% fee, or burn kVARA and unbond natively for free in seven days at the full rate.'],
  ['How do the rewards work?', 'kVARA is non-rebasing. Your balance stays fixed while the redemption rate rises every era as staking rewards compound, so there is nothing to claim.'],
  ['Is there a minimum amount to stake?', 'No. Vara requires 50 VARA to nominate, so the protocol pools smaller deposits and nominates them together.'],
  ['Do you support DAOs, validators or institutions?', 'Yes. Treasuries can hold kVARA like any token, and validators can join the curated set. Contact the team to discuss a custom setup.'],
];
export function FAQs() {
  return (
    <Section id="faq" className="fx-faqs">
      <Container>
        <div className="fx-split">
          <div className="fx-faqs-left">
            <div className="fx-top-left">
              <h2 className="fx-h2">Frequently asked questions</h2>
              <p className="fx-body">Find quick answers to common questions about the protocol, fees and security.</p>
            </div>
            <FaqsCta avatars={['gDcaZH5xt6hqSU2VbK2snAw.jpg', 'X0ECJ5xGgYrCVgHB8RYd3RABTQ.jpg', '622M5cyJBdKPIK1fPnBlo3qONk.jpg'].map((a) => FX + a)} />
          </div>
          <div className="fx-acc-list">
            {QS.map(([q, a], i) => <Accordion key={q} question={q} answer={a} open={i === 0} />)}
          </div>
        </div>
      </Container>
    </Section>
  );
}

/* ---------- Footer (CTA + card) ---------- */
export function Footer({ current }: { current?: string }) {
  const cols: [string, [string, string][]][] = [
    ['Quick links', [['/#features', 'Features'], ['/#how-it-works', 'How it works'], ['/#use-cases', 'Use cases'], ['/#integrations', 'Integrations']]],
    ['Pages', [['/features', 'Product'], ['/app', 'Stake'], ['/app/vaults', 'Vaults'], ['/app/portfolio', 'Portfolio']]],
    ['Support', [['/#faq', 'FAQs'], ['mailto:hello@valeprotocol.xyz', 'Contact'], ['https://wiki.vara.network/', 'Vara docs'], ['/#security', 'Security']]],
  ];
  return (
    <footer className="fx-sec fx-footer">
      <Container>
        <div className="fx-footer-content">
          <div className="fx-footer-cta">
            <div className="fx-top">
              <h2 className="fx-h2 fx-center">Ready to stake smarter?</h2>
              <p className="fx-body fx-center">Join stakers using kVARA to stay liquid, compound every era and put VARA to work across DeFi.</p>
            </div>
            <div className="fx-buttons">
              <BtnIcon to="/app">Start staking</BtnIcon>
              <Btn to="/features">Try the live demo</Btn>
            </div>
          </div>
          <div className="fx-footer-card">
            <div className="fx-footer-grid">
              <div className="fx-footer-brand">
                <div style={{ display: 'flex', flexDirection: 'column', gap: 20, alignItems: 'flex-start' }}>
                  <Wordmark size={30} />
                  <p className="fx-body">The liquid staking standard on Vara. Stake VARA, hold kVARA, stay liquid.</p>
                </div>
                <Btn variant="dark" href="mailto:hello@valeprotocol.xyz">hello@valeprotocol.xyz</Btn>
              </div>
              <div className="fx-footer-cols">
                {cols.map(([h, links]) => (
                  <div key={h} className="fx-footer-col">
                    <h4>{h}</h4>
                    <ul>{links.map(([href, l]) => <li key={href}><a href={href} aria-current={href === current ? 'page' : undefined} target={href.startsWith('http') ? '_blank' : undefined} rel={href.startsWith('http') ? 'noreferrer' : undefined}>{l}</a></li>)}</ul>
                  </div>
                ))}
              </div>
            </div>
            <div className="fx-footer-bar">
              <p>Built on <a href="https://vara.network/" target="_blank" rel="noreferrer">Vara Network</a>. Yield is embedded in the rate; staking involves slashing risk.</p>
              <div className="fx-socials">
                {SOCIALS.map(([href, label, path]) => (
                  <a key={href} href={href} className="fx-social" target="_blank" rel="noreferrer" aria-label={label}>
                    <svg width="14" height="14" viewBox="0 0 24 24" fill="currentColor" aria-hidden><path d={path} /></svg>
                  </a>
                ))}
              </div>
            </div>
          </div>
        </div>
      </Container>
      <div className="fx-bgitem" aria-hidden style={{ height: '100%' }}>
        <div className="fx-bg-overlay" style={{ height: '55%' }} />
        <img src={`${FX}Osh2UHQcarC8mZnL6oh6gwYnbA.jpg`} alt="" className="fx-bgimg" />
      </div>
    </footer>
  );
}
