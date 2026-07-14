import {
  useCallback,
  useMemo,
  useState,
  useSyncExternalStore,
  type ReactNode,
} from "react";

import { isDevHost } from "./dev-host";
import { isIdentityExpired } from "./expiry";
import {
  clearIdentity as clearStored,
  getIdentity,
  getServerSnapshot,
  setIdentity,
  subscribe,
} from "./session";
import type { SignedRequest } from "./signer";
import {
  clearThirdwebSession,
  completeEmailLogin,
  hasThirdwebClientId,
  initiateEmailLogin,
  makeInAppSigner,
  setThirdwebSession,
  socialLoginUrl,
  type ThirdwebAuthResult,
  type ThirdwebSocialProvider,
} from "./thirdweb";
import { startPhonePairing, type PhonePairSession } from "./pair";
import type { AuthIdentity, SignedFetchMetadata } from "./types";
import { detectWallets, hasWallet, selectWallet, type DetectedWallet } from "./wallet";

async function identityFromInApp(
  auth: Pick<ThirdwebAuthResult, "token" | "walletAddress">,
  opts?: { expirationMs?: number },
): Promise<AuthIdentity> {
  const { createIdentityWith } = await import("./identity");
  const signer = makeInAppSigner(auth);
  const next = await createIdentityWith(signer.address, signer.personalSign, opts);
  setThirdwebSession({ token: signer.token, address: signer.address });
  return next;
}

export type AuthStatus = "anonymous" | "connecting" | "connected" | "expired";

export type UseAuth = {
  identity: AuthIdentity | null;
  address: string | null;
  status: AuthStatus;
  isConnected: boolean;
  walletAvailable: boolean;
  detectedWallets: DetectedWallet[];
  inAppAvailable: boolean;
  error: string | null;
  connect: (opts?: {
    expirationMs?: number;
    walletRdns?: string;
  }) => Promise<AuthIdentity | null>;
  connectPhonePairing: (opts?: {
    expirationMs?: number;
  }) => Promise<PhonePairSession>;
  startEmailSignIn: (email: string) => Promise<void>;
  verifyEmailSignIn: (
    email: string,
    code: string,
    opts?: { expirationMs?: number },
  ) => Promise<AuthIdentity | null>;
  startSocialSignIn: (
    provider: ThirdwebSocialProvider,
    redirectUrl: string,
  ) => void;
  completeInAppSession: (
    auth: Pick<ThirdwebAuthResult, "token" | "walletAddress">,
    opts?: { expirationMs?: number },
  ) => Promise<AuthIdentity | null>;
  devHost: boolean;
  connectWithKey: (
    signerPrivateKey?: `0x${string}`,
    opts?: { expirationMs?: number },
  ) => Promise<AuthIdentity | null>;
  disconnect: () => void;
  sign: (
    method: string,
    url: string,
    metadata?: SignedFetchMetadata,
  ) => Promise<SignedRequest>;
  fetch: (
    url: string,
    init?: RequestInit & { metadata?: SignedFetchMetadata },
  ) => Promise<Response>;
};

export function useAuth(): UseAuth {
  const identity = useSyncExternalStore(
    subscribe,
    getIdentity,
    getServerSnapshot,
  );
  const [connecting, setConnecting] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const expired = !!identity && isIdentityExpired(identity);

  const connect = useCallback(
    async (opts?: { expirationMs?: number; walletRdns?: string }) => {
      setError(null);
      setConnecting(true);
      try {
        if (opts?.walletRdns) selectWallet(opts.walletRdns);
        const { createIdentity } = await import("./identity");
        const next = await createIdentity(opts);
        setIdentity(next);
        return next;
      } catch (err) {
        setError((err as Error)?.message ?? "Failed to connect wallet.");
        return null;
      } finally {
        setConnecting(false);
      }
    },
    [],
  );

  const connectPhonePairing = useCallback(
    async (opts?: { expirationMs?: number }) => {
      setError(null);
      let session: PhonePairSession;
      try {
        session = await startPhonePairing(opts);
      } catch (err) {
        setError((err as Error)?.message ?? "Couldn't start phone sign-in.");
        throw err;
      }
      return session;
    },
    [],
  );

  const startEmailSignIn = useCallback(async (email: string) => {
    setError(null);
    try {
      await initiateEmailLogin(email);
    } catch (err) {
      const msg = (err as Error)?.message ?? "Couldn't send the sign-in code.";
      setError(msg);
      throw err;
    }
  }, []);

  const completeInAppSession = useCallback(
    async (
      auth: Pick<ThirdwebAuthResult, "token" | "walletAddress">,
      opts?: { expirationMs?: number },
    ) => {
      setError(null);
      setConnecting(true);
      try {
        const next = await identityFromInApp(auth, opts);
        setIdentity(next);
        return next;
      } catch (err) {
        setError((err as Error)?.message ?? "Failed to finish sign-in.");
        return null;
      } finally {
        setConnecting(false);
      }
    },
    [],
  );

  const verifyEmailSignIn = useCallback(
    async (email: string, code: string, opts?: { expirationMs?: number }) => {
      setError(null);
      setConnecting(true);
      try {
        const result = await completeEmailLogin(email, code);
        const next = await identityFromInApp(result, opts);
        setIdentity(next);
        return next;
      } catch (err) {
        setError((err as Error)?.message ?? "That code didn't work.");
        return null;
      } finally {
        setConnecting(false);
      }
    },
    [],
  );

  const startSocialSignIn = useCallback(
    (provider: ThirdwebSocialProvider, redirectUrl: string) => {
      setError(null);
      if (typeof window !== "undefined") {
        window.location.href = socialLoginUrl(provider, redirectUrl);
      }
    },
    [],
  );

  const connectWithKey = useCallback(
    async (signerPrivateKey?: `0x${string}`, opts?: { expirationMs?: number }) => {
      setError(null);
      setConnecting(true);
      try {
        const { signInWithPrivateKey } = await import("./dev-identity");
        return await signInWithPrivateKey(signerPrivateKey, opts);
      } catch (err) {
        setError((err as Error)?.message ?? "Failed to sign in with key.");
        return null;
      } finally {
        setConnecting(false);
      }
    },
    [],
  );

  const disconnect = useCallback(() => {
    setError(null);
    clearThirdwebSession();
    clearStored();
    // The verified-wallet cookie is HttpOnly, so only the server can drop it;
    // without this, flag targeting keeps using the signed-out wallet.
    void fetch("/auth/callback", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ signout: true }),
      keepalive: true,
    }).catch(() => {});
  }, []);

  const sign = useCallback(
    async (method: string, url: string, metadata?: SignedFetchMetadata) => {
      const id = getIdentity();
      if (!id) throw new Error("Not connected.");
      const { signRequest } = await import("./signer");
      return signRequest(id, method, url, metadata ?? {});
    },
    [],
  );

  const boundFetch = useCallback(
    async (url: string, init?: RequestInit & { metadata?: SignedFetchMetadata }) => {
      const id = getIdentity();
      if (!id) throw new Error("Not connected.");
      const { signedFetch } = await import("./signer");
      return signedFetch(id, url, init);
    },
    [],
  );

  const status: AuthStatus = connecting
    ? "connecting"
    : expired
      ? "expired"
      : identity
        ? "connected"
        : "anonymous";

  return useMemo(
    () => ({
      identity: expired ? null : identity,
      address: expired ? null : (identity?.signer ?? null),
      status,
      isConnected: !!identity && !expired,
      walletAvailable: hasWallet(),
      detectedWallets: detectWallets(),
      inAppAvailable: hasThirdwebClientId(),
      error,
      connect,
      connectPhonePairing,
      startEmailSignIn,
      verifyEmailSignIn,
      startSocialSignIn,
      completeInAppSession,
      devHost: isDevHost(),
      connectWithKey,
      disconnect,
      sign,
      fetch: boundFetch,
    }),
    [
      identity,
      expired,
      status,
      error,
      connect,
      connectPhonePairing,
      startEmailSignIn,
      verifyEmailSignIn,
      startSocialSignIn,
      completeInAppSession,
      connectWithKey,
      disconnect,
      sign,
      boundFetch,
    ],
  );
}

export function AuthProvider({ children }: { children: ReactNode }) {
  return <>{children}</>;
}
