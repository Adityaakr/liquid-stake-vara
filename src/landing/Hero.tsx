import { TokenIcon } from '@/ui';
import { Btn, BtnIcon, Container, ListItem, PreTitle, Shot, Ticker } from './fx';

const FX = '/fx/';
const CLOUDS: [string, number, { left?: number | string; right?: number | string; top: number }][] = [
  ['Rorgfh4qpKNsZyFzGNQ9wt5C0i4.png', 350, { left: -140, top: 40 }],
  ['fLN6Wx8BsWTV2MkQDeC8mB2BQKA.png', 240, { right: 40, top: 120 }],
  ['lSZuKptayJeB4Xcw10qjE7IisQw.png', 350, { right: -160, top: 330 }],
];
const PARTNERS = ['Validators', 'DEXs', 'Wallets', 'Bridges', 'DAOs', 'Lending markets'];

export function Hero() {
  return (
    <header className="fx-sec fx-hero" id="top">
      <Container>
        <div className="fx-hero-content">
          <div className="fx-hero-top">
            <div style={{ display: 'flex', flexDirection: 'column', alignItems: 'center', gap: 20 }}>
              <h1 className="fx-hero-title">
                <span>Liquid</span>
                <span className="sr-only"> </span>
                <span className="fx-hero-tile" aria-hidden><TokenIcon token="kVARA" size={58} /></span>
                <span>Staking</span>
              </h1>
              <p className="fx-body fx-hero-sub">Stake VARA, receive kVARA, and keep your capital working across Vara DeFi with instant or native exits.</p>
            </div>
            <div className="fx-buttons">
              <BtnIcon to="/app">Start staking now</BtnIcon>
              <Btn to="/features">View product</Btn>
            </div>
            <div className="fx-hero-list">
              <ListItem variant="fit" icon="star">$1.2M+ total value locked</ListItem>
              <span className="fx-line" />
              <ListItem variant="fit" icon="shield">Audited &amp; insured</ListItem>
              <span className="fx-line" />
              <ListItem variant="fit" icon="zap">Instant exits</ListItem>
            </div>
          </div>
          <div className="fx-hero-bottom">
            <div className="fx-hero-shot"><Shot name="app-hero" alt="The Vale Protocol staking dashboard" width={1400} height={846} mobileHeight={760} /></div>
          </div>
        </div>
      </Container>
      {CLOUDS.map(([f, h, pos]) => <img key={f} src={FX + f} alt="" className="fx-cloud" style={{ height: h, ...pos }} aria-hidden />)}
      <div className="fx-hero-deco" aria-hidden><div><img src={`${FX}OH5Re0X1fnTabOLoEQYYNvYZWdQ.png`} alt="" /></div></div>
      <div className="fx-bgitem" aria-hidden style={{ height: 282 }}><img src={`${FX}QonQfzdUmEwRaww2TW9LW9ODvR0.jpg`} alt="" className="fx-bgimg" /></div>
      <div className="fx-hero-bgbottom" aria-hidden />
    </header>
  );
}

export function Clients() {
  return (
    <section className="fx-sec fx-clients" aria-label="Trusted by">
      <Container>
        <div className="fx-clients-in">
          <div className="fx-clients-fade fx-clients-fade-l" aria-hidden />
          <div className="fx-clients-fade fx-clients-fade-r" aria-hidden />
          <div className="fx-clients-pre"><PreTitle>Trusted by validators and DeFi teams on Vara</PreTitle></div>
          <Ticker gap={70} duration={45}>
            {PARTNERS.map((p) => <span key={p} className="fx-clientsoon"><span>{p}</span>coming soon</span>)}
          </Ticker>
          <div className="fx-clients-line" aria-hidden />
        </div>
      </Container>
    </section>
  );
}
