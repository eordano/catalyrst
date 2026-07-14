import type { ReactNode } from "react";
import ChromeShell from "../../components/ChromeShell";
import DclTopBar from "../../web/frames/DclTopBar";
import { useChromeAuth } from "../../web/frames/chrome-auth";
import SearchField from "../../atoms/SearchField";
import "./governancechrome.css";

export type GovernanceNavId = "home" | "proposals" | "projects" | "transparency";

export const GOV_TABS: { id: GovernanceNavId; label: string }[] = [
  { id: "home", label: "DAO Home" },
  { id: "proposals", label: "Proposals" },
  { id: "projects", label: "Projects" },
  { id: "transparency", label: "Transparency" },
];

type GovernanceChromeProps = {
  active?: GovernanceNavId | null;
  onTab?: (id: GovernanceNavId) => void;
  children?: ReactNode;
  signedIn?: boolean;
  mana?: string;
  account?: string;
  onSignIn?: () => void;
};

export default function GovernanceChrome({
  active = "proposals",
  onTab,
  children,
  signedIn,
  account,
  onSignIn,
}: GovernanceChromeProps) {
  const auth = useChromeAuth();
  const isIn = signedIn ?? auth.signedIn;
  const acct = account ?? auth.account;
  const signIn = onSignIn ?? auth.onSignIn;
  return (
    <ChromeShell
      className="gv"
      ariaLabel="Governance"
      topbar={<DclTopBar variant="dao" active="" signedIn={isIn} account={acct} onSignIn={signIn} />}
      tabs={GOV_TABS}
      active={active ?? undefined}
      onTab={onTab}
      tabsLabel="Governance sections"
      right={
        <div className="gv__search">
          <SearchField placeholder="Search..." />
        </div>
      }
    >
      {children}
    </ChromeShell>
  );
}

export function GovernanceChromeMaybe({
  chrome = true,
  children,
  ...rest
}: GovernanceChromeProps & { chrome?: boolean }) {
  if (!chrome) return <>{children}</>;
  return <GovernanceChrome {...rest}>{children}</GovernanceChrome>;
}
