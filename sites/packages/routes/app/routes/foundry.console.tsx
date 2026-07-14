import { Outlet, useLocation } from "react-router";

import FdConsoleLayout from "@ui/foundry/pages/FdConsoleLayout";

import "@ui/foundry/pages/fdconsole.css";

const TABS = ["bench", "trajectories", "costs"] as const;

type ConsoleTab = (typeof TABS)[number];

function activeTab(pathname: string): ConsoleTab {
  const tail = pathname.replace(/^\/foundry\/console\/?/, "").split("/")[0];
  return (TABS as readonly string[]).includes(tail)
    ? (tail as ConsoleTab)
    : "bench";
}

export default function FoundryConsole() {
  const location = useLocation();
  return (
    <FdConsoleLayout active={activeTab(location.pathname)}>
      <Outlet />
    </FdConsoleLayout>
  );
}
