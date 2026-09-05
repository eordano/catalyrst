import type { ReactNode } from "react";
import ChromeShell from "../../components/ChromeShell";
import DclTopBar from "../../web/frames/DclTopBar";
import { useChromeAuth } from "../../web/frames/chrome-auth";
import "./accountchrome.css";

type AccountChromeProps = {
  children?: ReactNode;
  signedIn?: boolean;
  mana?: string;
  account?: string;
  onSignIn?: () => void;
};

export default function AccountChrome({
  children,
  signedIn,
  account,
  onSignIn,
}: AccountChromeProps) {
  const auth = useChromeAuth();
  const isIn = signedIn ?? auth.signedIn;
  const acct = account ?? auth.account;
  const signIn = onSignIn ?? auth.onSignIn;
  return (
    <ChromeShell
      className="ac"
      ariaLabel="Account"
      subnav={false}
      topbar={<DclTopBar variant="sites" active="" signedIn={isIn} account={acct} onSignIn={signIn} />}
    >
      {children}
    </ChromeShell>
  );
}
