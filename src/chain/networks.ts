import type { NetworkId } from './types';

/** Vite injects `import.meta.env`; Node scripts (deploy, smoke) run without it. */
const env: Record<string, string | undefined> = (import.meta as unknown as { env?: Record<string, string | undefined> }).env ?? {};

/** Vaultera runs on Vara mainnet. The RPC endpoint is configurable through VITE_VARA_RPC. */
export const NETWORKS: Record<NetworkId, { label: string; rpc: string; ss58: number; explorer: string }> = {
  mainnet: {
    label: /127\.0\.0\.1|localhost/.test(env.VITE_VARA_RPC ?? '') ? 'Local node' : 'Vara mainnet',
    rpc: env.VITE_VARA_RPC ?? 'wss://rpc.vara.network',
    ss58: 137,
    explorer: 'https://vara.subscan.io',
  },
};

export const DEFAULT_NETWORK: NetworkId = 'mainnet';

/** Explorer link for a transaction hash, or undefined on networks without one (local nodes). */
export function txLink(network: NetworkId, hash: string): string | undefined {
  const n = NETWORKS[network];
  return /127\.0\.0\.1|localhost/.test(n.rpc) ? undefined : `${n.explorer}/extrinsic/${hash}`;
}
