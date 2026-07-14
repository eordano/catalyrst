import { useCallback, useEffect, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { useRouteLoaderData } from "react-router";

import { ChromeAuthContext } from "@ui/web/frames/chrome-auth";

import { useAuth } from "@data/lib/auth/index";
import { useProfileIdentity } from "@data/lib/auth/use-profile-name";
import { catalystBase } from "@data/lib/catalyst/client";
import { openSignIn } from "../auth/signin-store";

export default function ChromeAuthBridge({ children }: { children: ReactNode }) {
  const rootData = useRouteLoaderData("root") as
    | { wallet?: string | null; committee?: boolean }
    | undefined;
  const seedWallet = rootData?.wallet ?? "";
  const committee = rootData?.committee ?? false;

  const [mounted, setMounted] = useState(false);
  useEffect(() => {
    setMounted(true);
  }, []);

  const { isConnected, address, fetch: signedFetch } = useAuth();
  const effSignedIn = mounted ? isConnected : !!seedWallet;
  const effAddress = mounted ? (address ?? "") : seedWallet;

  const { name, avatarUrl } = useProfileIdentity(effAddress || null, effSignedIn);

  const fetchNotifications = useCallback(async (): Promise<unknown> => {
    const res = await signedFetch(`${catalystBase()}/notifications?limit=50`, {
      headers: { accept: "application/json" },
    });
    if (!res.ok) throw new Error(`Notifications returned ${res.status}`);
    return res.json() as Promise<unknown>;
  }, [signedFetch]);

  const value = useMemo(
    () => ({
      signedIn: effSignedIn,
      account: effAddress,
      mana: "",
      name,
      committee,
      avatarUrl: avatarUrl || undefined,
      onSignIn: openSignIn,
      fetchNotifications,
    }),
    [effSignedIn, effAddress, name, committee, avatarUrl, fetchNotifications],
  );

  return <ChromeAuthContext.Provider value={value}>{children}</ChromeAuthContext.Provider>;
}
