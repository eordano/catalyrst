import { useEffect, useRef, useState } from "react";
import { useSearchParams } from "react-router";

import "@ui/governance/pages/gvdebugadmin.css";

import DebugConsolePanels, {
  type DebugPanel,
} from "@features/components/admin/DebugConsolePanels";
import { loadDebugConsole } from "@data/lib/catalyst/governance/debug-console";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { track } from "@core/lib/telemetry/track";

import type { Route } from "./+types/admin.debug-console";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "admin/debug-console";

const DEFAULT_ASSIGNMENT: Assignment = {
  variant: "console",
  flags: { console: true },
  experimentKey: "admin_debug_console",
};

/**
 * `?authorized=1` is gone.
 *
 * It was never an access control. It was a query parameter the visitor sets,
 * checked by nobody, that turned a 17,640-byte SSR response into a 27,891-byte
 * one for the same anonymous caller -- revealing a wall of admin-looking forms
 * that made no HTTP calls at all.
 *
 * Both reads behind this page are genuinely public -- neither
 * `catalyrst-governance/src/handlers/health.rs:3` nor
 * `catalyrst-governance/src/handlers/read.rs:220` takes an auth extractor of
 * any kind -- so there is nothing here to gate. The page renders them
 * unconditionally and says so, and the "tools" section renders a permanent
 * unavailable state instead of buttons.
 *
 * It is removed rather than hidden: the parameter is no longer read anywhere,
 * so there is no state it can still reach.
 */
function parsePanel(raw: string | null): DebugPanel {
  return raw === "Debug" ? "Debug" : "Admin";
}

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const panel = parsePanel(url.searchParams.get("panel"));

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    DEFAULT_ASSIGNMENT,
  );

  const console = await loadDebugConsole({ signal: request.signal });

  const payload = { sid, assignment, panel, console };

  return wrap(payload);
}

export default function AdminDebugConsoleRoute({
  loaderData,
}: Route.ComponentProps) {
  const d = loaderData;
  const [searchParams, setSearchParams] = useSearchParams();

  const urlPanel = parsePanel(searchParams.get("panel"));
  const [panel, setPanel] = useState<DebugPanel>(urlPanel || d.panel);

  const ctx = {
    sid: d.sid,
    story: STORY,
    variant: d.assignment.variant,
    experimentKey: d.assignment.experimentKey,
  };

  const viewed = useRef(false);
  useEffect(() => {
    if (viewed.current) return;
    viewed.current = true;
    track("admin_debug_console_viewed", { panel }, ctx);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [d.sid]);

  function onPanelSwitch(next: DebugPanel) {
    if (next === panel) return;
    setPanel(next);
    track("admin_debug_panel_switched", { panel: next }, ctx);
    const sp = new URLSearchParams(searchParams);
    sp.set("panel", next);
    setSearchParams(sp, { preventScrollReset: true });
  }

  return (
    <DebugConsolePanels
      data={d.console}
      panel={panel}
      onPanelSwitch={onPanelSwitch}
    />
  );
}
