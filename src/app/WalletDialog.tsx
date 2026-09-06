import { useEffect } from 'react';
import { useAccount } from '@gear-js/react-hooks';
import { WalletModal } from '@gear-js/wallet-connect';
import { useStore } from '@/chain/store';

/**
 * The official Vara wallet modal. Wallet detection, connection and persistence live in the
 * official account provider; this component only mirrors its chosen account into the store.
 */
export function WalletDialog({ open, onClose }: { open: boolean; onClose: () => void }) {
  const { account } = useAccount();
  const { setExternalAccount } = useStore();

  useEffect(() => {
    setExternalAccount(account ? { address: account.address, name: account.meta.name, source: account.meta.source } : null);
  }, [account, setExternalAccount]);

  if (!open) return null;
  return <WalletModal theme="gear" close={onClose} />;
}
