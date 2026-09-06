import { defineConfig } from 'vitest/config';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';

export default defineConfig({
  plugins: [react()],
  resolve: { alias: { '@': fileURLToPath(new URL('./src', import.meta.url)) } },
  build: {
    target: 'es2022',
    rollupOptions: {
      output: {
        manualChunks(id: string) {
          if (id.includes('@polkadot') || id.includes('@gear-js') || id.includes('sails-js')) return 'chain';
          return undefined;
        },
      },
    },
  },
  test: {
    environment: 'jsdom',
    globals: true,
    setupFiles: ['./src/test/setup.ts'],
    css: false,
    // The Vara hooks bundle imports a CommonJS wallet shim; let Vite transform it under vitest.
    server: { deps: { inline: ['@gear-js/react-hooks', '@gear-js/wallet-connect', '@gear-js/vara-ui', '@gear-js/ui', '@polkadot/react-identicon', '@varan-wallet/varan-connect'] } },
  },
});
