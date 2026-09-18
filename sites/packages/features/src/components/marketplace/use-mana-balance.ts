import { useCallback, useEffect, useState } from "react";

import { fetchManaBalance } from "@data/lib/catalyst/marketplace/topup";

import type { ManaBalance } from "./mana-phase";

export function useManaBalance(signer: string): {
  balance: ManaBalance;
  refresh: () => void;
} {
  const [balance, setBalance] = useState<ManaBalance>(undefined);
  const [read, setRead] = useState(0);

  useEffect(() => {
    let cancelled = false;
    if (!signer) {
      setBalance(null);
      return;
    }
    setBalance(undefined);
    fetchManaBalance(signer)
      .then((bal) => {
        if (!cancelled) setBalance(bal);
      })
      .catch(() => {
        if (!cancelled) setBalance(null);
      });
    return () => {
      cancelled = true;
    };
  }, [signer, read]);

  const refresh = useCallback(() => setRead((n) => n + 1), []);
  return { balance, refresh };
}
