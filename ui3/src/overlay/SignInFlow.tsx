import { useCallback, useState } from "react";

import SignInModalView, {
  type SignInSocialProvider,
} from "../components/SignInModalView";

import { createIdentityWith } from "../data/auth/identity";
import { loginWithIdentity } from "../data/auth/engineLogin";
import {
  ThirdwebError,
  completeEmailLogin,
  hasThirdwebClientId,
  initiateEmailLogin,
  makeInAppSigner,
  socialLoginUrl,
} from "../data/auth/thirdweb";
import {
  connectWallet,
  detectWallets,
  hasWallet,
  personalSign,
  selectWallet,
} from "../data/auth/wallet";
import { startPhonePairing } from "../data/auth/pair";

export type SignInFlowProps = {
  onClose: () => void;
  onSignedIn?: () => void;
};

function socialRedirectUrl(): string {
  if (typeof window === "undefined") return "/";
  return window.location.href;
}

function errMessage(err: unknown): string {
  if (err instanceof ThirdwebError) return err.message;
  if (err instanceof Error) return err.message;
  return "Something went wrong. Please try again.";
}

export default function SignInFlow({ onClose, onSignedIn }: SignInFlowProps) {
  const [authError, setAuthError] = useState<string | null>(null);

  const onStartEmailSignIn = useCallback(async (email: string) => {
    setAuthError(null);
    try {
      await initiateEmailLogin(email);
    } catch (err) {
      setAuthError(errMessage(err));
      throw err;
    }
  }, []);

  const onVerifyEmailSignIn = useCallback(
    async (email: string, code: string) => {
      setAuthError(null);
      try {
        const auth = await completeEmailLogin(email, code);
        const signer = makeInAppSigner(auth);
        const identity = await createIdentityWith(
          signer.address,
          signer.personalSign,
        );
        if (!loginWithIdentity(identity)) {
          throw new Error("Failed to build identity (missing ephemeral link).");
        }
        return identity;
      } catch (err) {
        setAuthError(errMessage(err));
        return undefined;
      }
    },
    [],
  );

  const onSocialSignIn = useCallback((provider: SignInSocialProvider) => {
    window.location.href = socialLoginUrl(provider, socialRedirectUrl());
  }, []);

  const onConnectWallet = useCallback(async (walletRdns?: string) => {
    setAuthError(null);
    try {
      selectWallet(walletRdns ?? null);
      const address = await connectWallet();
      const identity = await createIdentityWith(address, (message) =>
        personalSign(message, address),
      );
      if (!loginWithIdentity(identity)) {
        throw new Error("Failed to build identity (missing ephemeral link).");
      }
      return identity;
    } catch (err) {
      setAuthError(errMessage(err));
      return undefined;
    }
  }, []);

  return (
    <SignInModalView
      onClose={onClose}
      onSignedIn={onSignedIn}
      inAppAvailable={hasThirdwebClientId()}
      walletAvailable={hasWallet()}
      wallets={detectWallets()}
      desktopShell={false}
      authError={authError}
      onStartEmailSignIn={onStartEmailSignIn}
      onVerifyEmailSignIn={onVerifyEmailSignIn}
      onSocialSignIn={onSocialSignIn}
      onConnectWallet={onConnectWallet}
      onPhonePair={() => startPhonePairing()}
      onBeginShellSignIn={() => ({
        identity: Promise.resolve(null),
        cancel: () => {},
      })}
    />
  );
}
