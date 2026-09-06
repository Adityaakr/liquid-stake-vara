/// <reference types="vite/client" />
interface ImportMetaEnv {
  readonly VITE_VARA_RPC?: string;
  readonly VITE_ADAPTER?: 'mock' | 'gear';
  readonly VITE_VALE_PROGRAM_ID?: string;
}
interface ImportMeta { readonly env: ImportMetaEnv }
