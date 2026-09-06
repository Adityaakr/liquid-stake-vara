import { createBrowserRouter, RouterProvider, ScrollRestoration, Outlet, useRouteError } from 'react-router';
import { AccountProvider } from '@gear-js/react-hooks';
import { StoreProvider } from '@/chain/store';
import { APP_NAME } from '@/chain/wallet';
import { LandingPage } from '@/landing/LandingPage';
import { FeaturesPage } from '@/landing/FeaturesPage';
import { AppLayout } from '@/app/AppLayout';
import { StakePage } from '@/app/StakePage';
import { VaultsPage } from '@/app/VaultsPage';
import { PortfolioPage } from '@/app/PortfolioPage';
import { SettingsPage } from '@/app/SettingsPage';

function Root() {
  return (
    <AccountProvider appName={APP_NAME}>
      <StoreProvider>
        <ScrollRestoration />
        <Outlet />
      </StoreProvider>
    </AccountProvider>
  );
}

function RouteError() {
  const err = useRouteError();
  const msg = err instanceof Error ? err.message : 'Something went wrong.';
  return (
    <div style={{ minHeight: '100vh', display: 'grid', placeItems: 'center', textAlign: 'center', padding: 24 }}>
      <div>
        <div className="eyebrow" style={{ color: 'var(--danger)', marginBottom: 12 }}>Error</div>
        <h1 style={{ fontSize: 32, fontWeight: 600 }}>The app hit a problem.</h1>
        <p className="mono" style={{ color: 'var(--text-2)', marginTop: 10, fontSize: 13 }}>{msg}</p>
        <p style={{ marginTop: 16 }}><a href="/app">Reload the app</a></p>
      </div>
    </div>
  );
}

function NotFound() {
  return (
    <div style={{ minHeight: '100vh', display: 'grid', placeItems: 'center', textAlign: 'center', padding: 24 }}>
      <div>
        <div className="eyebrow" style={{ color: 'var(--fx-indigo)', marginBottom: 12 }}>404</div>
        <h1 style={{ fontSize: 40, fontWeight: 600 }}>Nothing here.</h1>
        <p style={{ color: 'var(--text-2)', marginTop: 10 }}><a href="/">Back to Vale Protocol</a></p>
      </div>
    </div>
  );
}

export const routes = [
  {
    element: <Root />,
    errorElement: <RouteError />,
    children: [
      { path: '/', element: <LandingPage /> },
      { path: '/features', element: <FeaturesPage /> },
      {
        path: '/app',
        element: <AppLayout />,
        children: [
          { index: true, element: <StakePage /> },
          { path: 'vaults', element: <VaultsPage /> },
          { path: 'portfolio', element: <PortfolioPage /> },
          { path: 'settings', element: <SettingsPage /> },
        ],
      },
      { path: '*', element: <NotFound /> },
    ],
  },
];

const router = createBrowserRouter(routes);

export default function App() {
  return <RouterProvider router={router} />;
}
