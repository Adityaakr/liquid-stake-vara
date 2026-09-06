import { useEffect } from 'react';
import { Bell, Cpu, Globe, RefreshCw, ShieldCheck, Sparkles, Link2, ArrowLeftRight, Boxes, Coins, Layers, Wallet } from 'lucide-react';
import './landing.css';
import { LandingNav } from './Nav';
import { Clients } from './Hero';
import { Footer } from './Closing';
import { BgItem, BtnIcon, CapabilityCard, Container, IntegrationCard, ListItem, PreTitle, Section, Shot, StatItem, StepCard } from './fx';

const FX = '/fx/';

export function FeaturesPage() {
  useEffect(() => { document.title = 'Product — Vale Protocol'; window.scrollTo(0, 0); }, []);
  return (
    <div style={{ background: '#fff', overflow: 'clip' }}>
      <LandingNav />
      <main>
        <header className="fx-sec fx-phero">
          <Container>
            <div className="fx-phero-content">
              <div className="fx-phero-left">
                <div className="fx-top-left">
                  <h1 className="fx-h1">Liquid staking built for the whole portfolio</h1>
                  <p className="fx-body" style={{ maxWidth: 600 }}>A non-rebasing receipt token, two exit paths, and stable vaults that turn looping demand into yield.</p>
                </div>
                <div className="fx-phero-stats">
                  <StatItem size="sm" title="99.9%" description="Reliable access to your stake and exits." />
                  <StatItem size="sm" title="$1.2M+" description="Total value locked across the kVARA, kUSDT and kUSDC pools." />
                </div>
                <BtnIcon to="/app">Start staking now</BtnIcon>
              </div>
              <div className="fx-phero-shot"><Shot name="app-portfolio" alt="The Vale Protocol portfolio view" width={1400} height={800} mobileHeight={700} /></div>
            </div>
          </Container>
          {[['Rorgfh4qpKNsZyFzGNQ9wt5C0i4.png', 350, { left: -140, top: 40 }], ['fLN6Wx8BsWTV2MkQDeC8mB2BQKA.png', 240, { right: 40, top: 100 }], ['lSZuKptayJeB4Xcw10qjE7IisQw.png', 350, { right: -160, top: 330 }]].map(([f, h, pos]) => (
            <img key={f as string} src={FX + (f as string)} alt="" className="fx-cloud" style={{ height: h as number, ...(pos as object) }} aria-hidden />
          ))}
          <BgItem src={`${FX}QonQfzdUmEwRaww2TW9LW9ODvR0.jpg`} top />
        </header>
        <Clients />

        <Section className="fx-pad-t">
          <Container>
            <div className="fx-split">
              <div className="fx-imgpanel"><img src={`${FX}ixHeRnAI0qhzdoCm4eL8vAFx0.png`} alt="" /></div>
              <div className="fx-split-col">
                <div className="fx-top-left">
                  <PreTitle>Under the hood</PreTitle>
                  <h2 className="fx-h2">A rate that works<br />while you sleep</h2>
                  <div className="fx-split-list" style={{ paddingTop: 10 }}>
                    <ListItem>Rewards compound into the rate every era</ListItem>
                    <ListItem>Nominations spread across a screened validator set</ListItem>
                    <ListItem>Buffer refills automatically from new deposits</ListItem>
                    <ListItem>Every parameter is public and on chain</ListItem>
                  </div>
                </div>
                <BtnIcon variant="dark" to="/app">Get started now</BtnIcon>
              </div>
            </div>
          </Container>
        </Section>

        <Section className="fx-caps">
          <Container>
            <div className="fx-top" style={{ maxWidth: 800, margin: '0 auto' }}>
              <PreTitle>Capabilities</PreTitle>
              <h2 className="fx-h2 fx-center">Everything inside Vale Protocol</h2>
            </div>
            <div className="fx-caps-grid">
              <CapabilityCard icon={<Cpu size={24} />} title="Non-rebasing kVARA" description="A fixed balance and a rising redemption rate compose with every DEX, bridge and market." />
              <CapabilityCard icon={<ShieldCheck size={24} />} title="Conservative risk engine" description="50% max LTV, 65% liquidation threshold and an insurance fund that fills first." />
              <CapabilityCard icon={<RefreshCw size={24} />} title="Two exit paths" description="Swap instantly from the buffer or DEX, or unbond natively in seven days at the full rate." />
              <CapabilityCard icon={<Bell size={24} />} title="Smart alerts" description="Get notified when the buffer drains, utilization nears the kink or an unbond is claimable." />
              <CapabilityCard icon={<Sparkles size={24} />} title="Stable vaults" description="Deposit wUSDT or wUSDC and earn the borrow interest that loopers pay." />
              <CapabilityCard icon={<Globe size={24} />} title="Curated validators" description="Nominations are screened for commission, uptime and slash history, never blind." />
            </div>
          </Container>
          <BgItem src={`${FX}cQBpXWVe2IileA0HCzW5W4uvgY.jpg`} top bottom />
        </Section>

        <Section className="fx-pad-b">
          <Container>
            <div className="fx-top" style={{ maxWidth: 800, margin: '0 auto' }}>
              <PreTitle>How it works</PreTitle>
              <h2 className="fx-h2 fx-center">Start staking in minutes</h2>
              <p className="fx-body fx-center">Connect a wallet, stake VARA and let the redemption rate do the rest while you stay liquid.</p>
            </div>
            <div className="fx-stepcards">
              <StepCard number="01" title="Connect your wallet" description="Link any Substrate wallet in one click. Your keys never leave it." />
              <StepCard number="02" title="Stake and mint kVARA" description="Deposit VARA and receive kVARA at the live rate." variant="primary" />
              <StepCard number="03" title="Earn every era" description="Rewards compound into the rate. Exit instantly or unbond natively." variant="dark" />
            </div>
          </Container>
        </Section>

        <Section>
          <Container>
            <div className="fx-split">
              <div className="fx-split-col">
                <div className="fx-top-left">
                  <PreTitle>Under the hood</PreTitle>
                  <h2 className="fx-h2">Risk analysis that<br />goes deeper</h2>
                  <p className="fx-body">Most protocols show you what happened. Vale Protocol stress-tests every parameter against slashes, price gaps and utilization spikes before they reach you.</p>
                </div>
                <div className="fx-split-list">
                  <ListItem>Simulate slashes and price shocks against the fund</ListItem>
                  <ListItem>Track utilization against the 80% kink</ListItem>
                  <ListItem>Evaluate buffer depth against exit demand</ListItem>
                  <ListItem>Forecast liquidation bands per position</ListItem>
                </div>
              </div>
              <div className="fx-imgpanel"><img src={`${FX}ec4uQ2RRHVbYypFsBUsSov3augQ.png`} alt="" /></div>
            </div>
          </Container>
        </Section>

        <Section className="fx-pad-t" style={{ paddingBottom: 100 }}>
          <Container>
            <div className="fx-top" style={{ maxWidth: 800, margin: '0 auto 50px' }}>
              <PreTitle variant="white">Integrations</PreTitle>
              <h2 className="fx-h2 fx-center">Works with the tools you already use</h2>
              <p className="fx-body fx-center">Connect your wallet and the Vara ecosystem to put kVARA to work instantly.</p>
            </div>
            <div className="fx-integrations-grid">
              <IntegrationCard icon={<Wallet size={24} color="#406AE4" />} title="Wallets" description="Any Substrate wallet extension, keys never leave it." />
              <IntegrationCard icon={<ArrowLeftRight size={24} color="#10B981" />} title="DEX" description="Instant kVARA exits backstopped by the DEX." />
              <IntegrationCard icon={<Link2 size={24} color="#5290F4" />} title="Bridges" description="Bring wUSDT and wUSDC in, take yield out." />
              <IntegrationCard icon={<Boxes size={24} color="#FF8B06" />} title="Lending" description="kVARA is the only collateral in the loop market." />
              <IntegrationCard icon={<Coins size={24} color="#FDBB6E" />} title="Treasuries" description="Hold kVARA like any token with full reporting." />
              <IntegrationCard icon={<Layers size={24} color="#1D1D1D" />} title="Validators" description="Join the curated set and grow with the pool." />
            </div>
          </Container>
        </Section>
      </main>
      <Footer current="/features" />
    </div>
  );
}
