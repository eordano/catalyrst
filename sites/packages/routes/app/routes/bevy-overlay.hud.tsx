import { memo, useCallback, useMemo, useState } from "react";
import type { ShouldRevalidateFunctionArgs } from "react-router";
import { useNavigate, useSearchParams } from "react-router";

import { getJSON } from "@data/lib/catalyst/client";
import {
  RealmAboutSchema,
  type RealmAbout,
} from "@data/lib/catalyst/creator-hub/activity";
import { loadBackpackEmotes } from "@data/lib/catalyst/overlay/backpack-emotes.server";
import {
  isEthAddress,
  normalizeAddress,
  type BackpackEmotesData,
} from "@data/lib/catalyst/overlay/backpack-emotes";
import { type Assignment } from "@core/lib/experiments/assign";
import { storyLoader } from "@core/lib/experiments/story-loader";
import ClientStage from "@ui/overlay/panels/ClientStage";
import Sidebar from "@ui/explorer/frames/Sidebar";
import Minimap from "@ui/explorer/frames/Minimap";
import Chat from "@ui/explorer/frames/ChatBridge";
import VoiceChat from "@ui/explorer/components/VoiceChat";
import SkyboxHUD from "@ui/explorer/components/SkyboxHUD";
import EmoteWheel from "@ui/explorer/components/EmoteWheel";
import ProfileWidget from "@ui/explorer/components/ProfileWidget";
import ConnectionStatus from "@ui/explorer/components/ConnectionStatus";
import EngineToasts from "@ui/explorer/components/EngineToasts";
import LoginCodeModal from "@ui/explorer/components/LoginCodeModal";
import PermissionPrompt from "@ui/explorer/components/PermissionPrompt";
import { useBridgeState, sendBridge } from "@ui/overlay/bridge";
import { MinimapVisibilityProvider } from "@ui/overlay/minimapVisibility";
import "@ui/overlay/overlay.css";

import type { Route } from "./+types/bevy-overlay.hud";
import type { StoryId } from "@core/lib/telemetry/story-id";

const STORY: StoryId = "client/hud-overlay";

type WidgetId = "profile" | "connection";

function parseWidget(raw: string | null): WidgetId | null {
  if (raw === "profile" || raw === "connection") return raw;
  return null;
}

const FALLBACK: Assignment = {
  variant: "overlay",
  flags: { domOverlay: true },
  experimentKey: "client_hud_overlay",
};

export async function loader({ request }: Route.LoaderArgs) {
  const url = new URL(request.url);
  const widget = parseWidget(url.searchParams.get("widget"));
  const rawAddr = url.searchParams.get("address");
  const address =
    rawAddr && isEthAddress(rawAddr) ? normalizeAddress(rawAddr) : null;

  const { sid, assignment, wrap } = await storyLoader(
    request,
    STORY,
    FALLBACK,
  );

  let realm: RealmAbout | null = null;
  try {
    const raw = await getJSON<unknown>("/about", { signal: request.signal });
    const parsed = RealmAboutSchema.safeParse(raw);
    realm = parsed.success ? parsed.data : null;
  } catch {
    realm = null;
  }

  const emotes = await loadBackpackEmotes(address, {
    signal: request.signal,
  }).catch(() => loadBackpackEmotes(null));

  const payload = { sid, widget, assignment, realm, emotes };

  return wrap(payload);
}

export function shouldRevalidate({
  currentUrl,
  nextUrl,
  defaultShouldRevalidate,
}: ShouldRevalidateFunctionArgs) {
  if (
    currentUrl.pathname === nextUrl.pathname &&
    currentUrl.searchParams.get("address") ===
      nextUrl.searchParams.get("address")
  ) {
    return false;
  }
  return defaultShouldRevalidate;
}

export default function HudRoute({ loaderData }: Route.ComponentProps) {
  const d = loaderData;
  return <HudStage realm={d.realm} emotes={d.emotes} />;
}

type LeftPanelId = "voice" | "skybox";

type StageProps = {
  realm: RealmAbout | null;
  emotes: BackpackEmotesData;
};

const HudMinimap = memo(function HudMinimap({
  place,
  fallbackCoords,
}: {
  place: string | undefined;
  fallbackCoords: string | null;
}) {
  const playerPosition = useBridgeState((s) => s.playerPosition);
  return (
    <div className="ui3-overlay__widget ui3-overlay__minimap">
      <Minimap
        place={place}
        coords={playerPosition?.parcel ?? fallbackCoords ?? undefined}
        heading={playerPosition?.heading}
      />
    </div>
  );
});

function HudStage({ realm, emotes }: StageProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();
  const widget = parseWidget(searchParams.get("widget"));

  const identity = useBridgeState((s) => s.identity);
  const live = useBridgeState((s) => s.live);
  const avatarPreview = useBridgeState((s) => s.avatarPreview);
  const scene = useBridgeState((s) => s.scene);
  const connection = useBridgeState((s) => s.connection);
  const toasts = useBridgeState((s) => s.toasts);
  const avatarLoadout = useBridgeState((s) => s.avatarLoadout);

  const emoteLoadout = useMemo(
    () =>
      avatarLoadout?.emotes?.length
        ? avatarLoadout.emotes.flatMap((urn, slot) =>
            urn && slot <= 9 ? [{ slot, urn, name: null }] : [],
          )
        : emotes.loadout,
    [avatarLoadout, emotes.loadout],
  );

  const [chatOpen, setChatOpen] = useState(false);
  const [emoteOpen, setEmoteOpen] = useState(false);
  const [leftPanel, setLeftPanel] = useState<LeftPanelId | null>(null);
  const toggleLeft = useCallback(
    (id: LeftPanelId) => setLeftPanel((p) => (p === id ? null : id)),
    [],
  );

  const setWidget = useCallback(
    (next: WidgetId | null) => {
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          if (next) params.set("widget", next);
          else params.delete("widget");
          return params;
        },
        { preventScrollReset: true },
      );
    },
    [setSearchParams],
  );

  const onProfileToggle = useCallback(
    () => setWidget(widget === "profile" ? null : "profile"),
    [widget, setWidget],
  );
  const onConnectionToggle = useCallback(
    () => setWidget(widget === "connection" ? null : "connection"),
    [widget, setWidget],
  );

  const address = searchParams.get("address");
  const onCustomise = useCallback(() => {
    navigate(
      address
        ? `/bevy-overlay/backpack-emotes?address=${encodeURIComponent(address)}`
        : "/bevy-overlay/backpack-emotes",
    );
  }, [address, navigate]);

  const realmName = scene.realm ?? realm?.configurations?.realmName ?? null;
  const health =
    connection == null
      ? "info"
      : connection.globalRoom && connection.sceneHealth === "ok"
        ? "ok"
        : "warn";

  return (
    <ClientStage nojs="Enable JavaScript to use the in-world HUD.">
      <MinimapVisibilityProvider>
        <div className="ui3-overlay" data-live={live ? "true" : "false"}>
          <div className="ui3-overlay__widget ui3-overlay__sidebar">
            <Sidebar
              avatarPreview={avatarPreview}
              onProfileToggle={onProfileToggle}
              chatOpen={chatOpen}
              onChatToggle={() => setChatOpen((o) => !o)}
              voiceOpen={leftPanel === "voice"}
              onVoiceToggle={() => toggleLeft("voice")}
              skyboxOpen={leftPanel === "skybox"}
              onSkyboxToggle={() => toggleLeft("skybox")}
              emoteOpen={emoteOpen}
              onEmoteToggle={() => setEmoteOpen((o) => !o)}
            />
          </div>
          {!leftPanel && (
            <HudMinimap
              place={scene.title ?? realmName ?? undefined}
              fallbackCoords={scene.coords}
            />
          )}
          <div className="ui3-overlay__widget ui3-overlay__profile">
            <ProfileWidget
              open={widget === "profile"}
              name={identity.name}
              tag={identity.tag ?? undefined}
              wallet={identity.wallet ?? undefined}
              address={identity.address ?? undefined}
              avatarSrc={avatarPreview}
              isGuest={identity.isGuest}
              onSignOut={
                identity.isGuest ? undefined : () => sendBridge("Logout", {})
              }
              onClose={onProfileToggle}
            />
          </div>
          <div className="ui3-overlay__widget ui3-overlay__chat">
            <Chat
              open={chatOpen}
              onToggle={() => setChatOpen((o) => !o)}
              hidden={leftPanel != null}
            />
          </div>
          {leftPanel === "voice" && (
            <div className="ui3-overlay__widget ui3-overlay__voice">
              <VoiceChat bare />
            </div>
          )}
          {leftPanel === "skybox" && (
            <div className="ui3-overlay__widget ui3-overlay__skybox">
              <SkyboxHUD />
            </div>
          )}
          {emoteOpen && (
            <div className="ui3-overlay__widget ui3-overlay__emote">
              <EmoteWheel
                catalog={emotes.catalog}
                loadout={emoteLoadout}
                onSelect={() => setEmoteOpen(false)}
                onClose={() => setEmoteOpen(false)}
                onCustomise={onCustomise}
              />
            </div>
          )}
          <div className="ui3-overlay__widget ui3-overlay__connbadge">
            <button
              type="button"
              className={"connbadge connbadge--" + health}
              aria-label="Connection status"
              aria-expanded={widget === "connection"}
              title="Connection status"
              onClick={onConnectionToggle}
            >
              <span className="connbadge__dot" />
            </button>
          </div>
          {widget === "connection" && (
            <div className="ui3-overlay__widget ui3-overlay__connection">
              <ConnectionStatus
                connection={connection}
                realm={realmName}
                onClose={onConnectionToggle}
              />
            </div>
          )}
          <EngineToasts toasts={toasts} />
        </div>
      </MinimapVisibilityProvider>
      <LoginCodeModal />
      <PermissionPrompt />
    </ClientStage>
  );
}
