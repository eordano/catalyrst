import { useEffect } from "react";
import { useNavigate } from "react-router";

import SignInModalView, {
  type SignInSocialProvider,
} from "@ui/components/SignInModalView";

import { useAuth } from "@data/lib/auth/index";
import {
  beginShellBrowserSignIn,
  isDesktopShell,
} from "@data/lib/auth/native-shell";
import { closeSignIn, takePendingRedirect, useSignInOpen } from "./signin-store";

function socialRedirectUrl(): string {
  if (typeof window === "undefined") return "/auth/callback";
  const next = window.location.pathname + window.location.search;
  return `${window.location.origin}/auth/callback?next=${encodeURIComponent(next)}`;
}

export default function SignInModalHost() {
  const open = useSignInOpen();
  const auth = useAuth();
  const { isConnected } = auth;
  const navigate = useNavigate();

  useEffect(() => {
    if (open && isConnected) {
      closeSignIn();
      const dest = takePendingRedirect();
      if (dest) navigate(dest);
    }
  }, [open, isConnected, navigate]);

  if (!open || isConnected) return null;
  return (
    <SignInModalView
      onClose={closeSignIn}
      inAppAvailable={auth.inAppAvailable}
      walletAvailable={auth.walletAvailable}
      wallets={auth.detectedWallets}
      desktopShell={isDesktopShell()}
      authError={auth.error}
      onStartEmailSignIn={(email) => auth.startEmailSignIn(email)}
      onVerifyEmailSignIn={(email, code) => auth.verifyEmailSignIn(email, code)}
      onSocialSignIn={(provider: SignInSocialProvider) =>
        auth.startSocialSignIn(provider, socialRedirectUrl())
      }
      onConnectWallet={(walletRdns) => auth.connect({ walletRdns })}
      onPhonePair={() => auth.connectPhonePairing()}
      onBeginShellSignIn={beginShellBrowserSignIn}
    />
  );
}
