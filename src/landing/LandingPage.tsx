import { useEffect } from 'react';
import './landing.css';
import { LandingNav } from './Nav';
import { Clients, Hero } from './Hero';
import { Comparison, Features, Integrations, Overview, Security, Stats, Steps, Testimonials, UseCases } from './Sections';
import { FAQs, Footer, Pricing } from './Closing';

export function LandingPage() {
  useEffect(() => { document.title = 'Vale Protocol — Liquid staking on Vara'; }, []);
  return (
    <div style={{ background: '#fff', overflow: 'clip' }}>
      <LandingNav />
      <main>
        <Hero />
        <Clients />
        <Comparison />
        <Features />
        <Overview />
        <Steps />
        <Security />
        <UseCases />
        <Integrations />
        <Stats />
        <Testimonials />
        <Pricing />
        <FAQs />
      </main>
      <Footer />
    </div>
  );
}
