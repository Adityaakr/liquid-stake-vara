import type { HexString } from '@gear-js/api';
import { VAULT_ASSETS, type VaultAsset } from '@/domain/protocol';

export type ProgramSet = { token: HexString; vault: HexString };

const HEX32 = /^0x[0-9a-fA-F]{64}$/;
/** Vite injects `import.meta.env`; Node scripts (deploy, smoke) run without it. */
const viteEnv: Record<string, string | undefined> = (import.meta as unknown as { env?: Record<string, string | undefined> }).env ?? {};

function readId(key: string): HexString | undefined {
  const v = viteEnv[key]?.trim();
  return v && HEX32.test(v) ? (v as HexString) : undefined;
}

/**
 * Program ids per asset from the environment. Missing or malformed ids leave the asset
 * undeployed; the UI shows that state instead of simulating anything.
 */
export function readPrograms(env: Partial<Record<string, string>> = {}): Partial<Record<VaultAsset, ProgramSet>> {
  const out: Partial<Record<VaultAsset, ProgramSet>> = {};
  for (const asset of VAULT_ASSETS) {
    const token = env[`VITE_${asset}_TOKEN`] && HEX32.test(env[`VITE_${asset}_TOKEN`]!) ? (env[`VITE_${asset}_TOKEN`] as HexString) : readId(`VITE_${asset}_TOKEN`);
    const vault = env[`VITE_K${asset}_VAULT`] && HEX32.test(env[`VITE_K${asset}_VAULT`]!) ? (env[`VITE_K${asset}_VAULT`] as HexString) : readId(`VITE_K${asset}_VAULT`);
    if (token && vault) out[asset] = { token, vault };
  }
  return out;
}

/** Gas limits for messages to the programs. Unused gas is refunded by the runtime. */
export const GAS = {
  /**
   * Vault commands that call the token program (deposit, redeem, claim). The program itself
   * requires 40B for one token call and 70B for two; the measured cost of a deposit was the
   * same at a 60B and a 100B limit, so the larger margin is free.
   */
  vaultAsync: 100_000_000_000n,
  /** Vault commands that stay inside the program (request unbond, accrue, admin). */
  vaultSync: 40_000_000_000n,
  /** Token commands (approve, faucet claim, transfer). */
  token: 30_000_000_000n,
} as const;
