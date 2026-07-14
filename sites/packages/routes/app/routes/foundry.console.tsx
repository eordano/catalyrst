import { Outlet, useLocation } from "react-router";

import FdConsoleLayout, {
  CONSOLE_TABS,
  type FdConsoleTab,
} from "@ui/foundry/pages/FdConsoleLayout";

import "@ui/foundry/pages/fdconsole.css";

function activeTab(pathname: string): FdConsoleTab | null {
  const tail = pathname.replace(/^\/foundry\/console\/?/, "").split("/")[0];
  return CONSOLE_TABS.find((tab) => tab.id === tail)?.id ?? null;
}

export default function FoundryConsole() {
  const location = useLocation();
  return (
    <FdConsoleLayout active={activeTab(location.pathname)}>
      <Outlet />
    </FdConsoleLayout>
  );
}
