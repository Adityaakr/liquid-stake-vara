import type { NetworkId } from './types';

/** Vale Protocol runs on Vara mainnet. The RPC endpoint is configurable through VITE_VARA_RPC. */
export const NETWORKS: Record<NetworkId, { label: string; rpc: string; ss58: number; explorer: string }> = {
  mainnet: {
    label: 'Vara mainnet',
    rpc: import.meta.env.VITE_VARA_RPC ?? 'wss://rpc.vara.network',
    ss58: 137,
    explorer: 'https://vara.subscan.io',
  },
};

export const DEFAULT_NETWORK: NetworkId = 'mainnet';
