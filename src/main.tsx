import { StrictMode } from 'react';
import { createRoot } from 'react-dom/client';
// Official Vara component styles first, so the site's own tokens and fonts win where they overlap.
import '@gear-js/ui/dist/index.css';
import '@gear-js/vara-ui/dist/style.css';
import '@gear-js/wallet-connect/dist/style.css';
import '@/styles/global.css';
import '@/ui/ui.css';
import App from './App';

createRoot(document.getElementById('root')!).render(
  <StrictMode>
    <App />
  </StrictMode>,
);
