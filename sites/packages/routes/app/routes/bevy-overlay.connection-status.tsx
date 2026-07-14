import { useCallback } from "react";
import type { ShouldRevalidateFunctionArgs } from "react-router";
import { useSearchParams } from "react-router";

import { getJSON } from "@data/lib/catalyst/client";
import { RealmAboutSchema } from "@data/lib/catalyst/creator-hub/activity";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import ClientStage from "@ui/overlay/panels/ClientStage";
import ConnectionStatus from "@ui/explorer/components/ConnectionStatus";
import { useBridgeState } from "@ui/overlay/bridge";
import "@ui/overlay/overlay.css";

import type { Route } from "./+types/bevy-overlay.connection-status";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/hud-overlay";

const FALLBACK: Assignment = {
  variant: "overlay",
  flags: { domOverlay: true },
  experimentKey: "client_hud_overlay",
};

type RealmStatus = {
  realmName: string | null;
  commsProtocol: string | null;
  usersCount: number | null;
  unavailable: boolean;
};

const UNAVAILABLE_REALM: RealmStatus = {
  realmName: null,
  commsProtocol: null,
  usersCount: null,
  unavailable: true,
};

function parseComms(raw: unknown): { protocol: string | null; usersCount: number | null } {
  const none = { protocol: null, usersCount: null };
  if (raw === null || typeof raw !== "object") return none;
  const comms = (raw as { comms?: unknown }).comms;
  if (comms === null || comms === undefined || typeof comms !== "object") return none;
  const c = comms as { protocol?: unknown; usersCount?: unknown };
  return {
    protocol: typeof c.protocol === "string" && c.protocol ? c.protocol : null,
    usersCount:
      typeof c.usersCount === "number" && Number.isFinite(c.usersCount)
        ? c.usersCount
        : null,
  };
}

function parseRealmStatus(raw: unknown): RealmStatus {
  const parsed = RealmAboutSchema.safeParse(raw);
  const comms = parseComms(raw);
  return {
    realmName: parsed.success ? parsed.data.configurations?.realmName ?? null : null,
    commsProtocol: comms.protocol,
    usersCount: comms.usersCount,
    unavailable: false,
  };
}

function isPanelOpen(raw: string | null): boolean {
  return raw === null || raw === "connection-status";
}

export async function loader({ request }: Route.LoaderArgs) {
  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let realm: RealmStatus = UNAVAILABLE_REALM;
  try {
    realm = parseRealmStatus(
      await getJSON<unknown>("/about", { signal: request.signal }),
    );
  } catch {
    realm = UNAVAILABLE_REALM;
  }

  const payload = { sid, realm, assignment };

  return wrap(payload);
}

export function shouldRevalidate({
  currentUrl,
  nextUrl,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  if (currentUrl.pathname === nextUrl.pathname) return false;
  return defaultShouldRevalidate;
}

export default function ConnectionStatusRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <ConnectionStatusStage realm={d.realm} />;
}

type StageProps = {
  realm: RealmStatus;
};

function ConnectionStatusStage({ realm }: StageProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const open = isPanelOpen(searchParams.get("panel"));

  const live = useBridgeState((s) => s.live);
  const connection = useBridgeState((s) => s.connection);
  const sceneRealm = useBridgeState((s) => s.scene.realm);

  const realmName = sceneRealm ?? realm.realmName;
  const health =
    connection == null
      ? "info"
      : connection.globalRoom && connection.sceneHealth === "ok"
        ? "ok"
        : "warn";

  const onToggle = useCallback(() => {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", open ? "closed" : "connection-status");
        return params;
      },
      { preventScrollReset: true },
    );
  }, [open, setSearchParams]);

  const onClose = useCallback(() => {
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        params.set("panel", "closed");
        return params;
      },
      { preventScrollReset: true },
    );
  }, [setSearchParams]);

  return (
    <ClientStage nojs="Enable JavaScript to inspect the live connection.">
      <div className="ui3-overlay" data-live={live ? "true" : "false"}>
        <div className="bv-overlay__conn">
          <div className="bv-overlay__realm" aria-live="polite">
            <span className="bv-overlay__realmname">
              {realmName ?? (realm.unavailable ? "Realm unavailable" : "Unknown realm")}
            </span>
            {realm.commsProtocol != null && (
              <span className="bv-overlay__realmmeta">
                comms {realm.commsProtocol}
                {realm.usersCount != null ? ` \u{B7} ${realm.usersCount} online` : ""}
              </span>
            )}
          </div>
        </div>
        <div className="ui3-overlay__widget ui3-overlay__connbadge">
          <button
            type="button"
            className={"connbadge connbadge--" + health}
            aria-label="Connection status"
            aria-expanded={open}
            title="Connection status"
            onClick={onToggle}
          >
            <span className="connbadge__dot" />
          </button>
        </div>
        {open && (
          <div className="ui3-overlay__widget ui3-overlay__connection">
            <ConnectionStatus
              connection={connection}
              realm={realmName}
              onClose={onClose}
            />
          </div>
        )}
      </div>
    </ClientStage>
  );
}
