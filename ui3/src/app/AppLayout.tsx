import { AudioMixerProvider } from "../overlay/audioMixer";
import type {
  MouseEvent as ReactMouseEvent,
  PointerEvent as ReactPointerEvent,
  SyntheticEvent,
} from "react";
import { lazy, Suspense, useCallback, useEffect, useRef, useState } from "react";
import { Outlet, useLocation, useNavigate } from "react-router";
import { useQueryClient } from "@tanstack/react-query";
import type { QueryClient } from "@tanstack/react-query";

import ExploreChrome, { EXPLORE_TABS } from "../explorer/frames/ExploreChrome";
import type { TabId } from "../explorer/frames/ExploreChrome";
import Sidebar from "../explorer/frames/Sidebar";
import SidebarDesign, { type SidebarDrawer } from "../explorer/frames/SidebarDesign";
import { sidebarDesignEnabled, SIDEBAR_DESIGN_FLAG } from "../data/sidebarDesignFlag";
import { flagState } from "../data/featureFlags";
import Minimap from "../explorer/frames/Minimap";
import Chat from "../explorer/frames/ChatBridge";
import { CHAT_PROFILE_STATE, ChatProfileContext, chatProfilePath, isChatProfile, type ViewChatProfile } from "../explorer/frames/chatProfile";
import { useNotifications } from "../data/hooks/useNotifications";
import { useOwnedEmotes } from "../data/hooks/useOwnedItems";
import { VoiceControls } from "../explorer/components/VoiceChat";
import EmoteWheel from "../explorer/components/EmoteWheel";
import FloatingPanel, { type FloatingPanelId } from "../explorer/components/FloatingPanel";
import JumpLoading, { JumpCompleteContext, usePanelJumpActive } from "../explorer/components/JumpLoading";
import { SkyboxControls } from "../explorer/components/SkyboxHUD";
import ProfileWidget from "../explorer/components/ProfileWidget";
import ConnectionStatus from "../explorer/components/ConnectionStatus";
import GraphicsProgress from "../explorer/components/GraphicsProgress";
import EngineToasts from "../explorer/components/EngineToasts";
import LoginCodeModal from "../explorer/components/LoginCodeModal";
import PermissionPrompt from "../explorer/components/PermissionPrompt";
import ShellLoading from "./ShellLoading";
import PanelBoundary from "./PanelBoundary";
import { useWorldEntry } from "./WorldEntry";
import { useBridgeState, sendBridge, stopEmote } from "../overlay/bridge";
import { signOutEngineAuth } from "../data/auth/engineLogin";
import { MinimapVisibilityProvider } from "../overlay/minimapVisibility";
import {
  isSynthesizedWorldKey,
  MobileHudFrame,
  OrientationProvider,
  TouchControls,
  useIsMobile,
  useViewportOrientation,
  WORLD_CANVAS_ID,
} from "../explorer/mobile";
import "../explorer/mobile/layout/viewport.css";
import "../overlay/overlay.css";
import "../explorer/pages/lobbyhome.css";
import "../explorer/frames/sidebar-popovers.css";
import { useLoadingTransfers } from "../overlay/loadingTransfers";

type LeftPanelId = "notif" | "voice" | "skybox" | "portables" | "friends";

const NotificationsPanel = lazy(() => import("./panels/Notifications.route"));
const FriendsPanel = lazy(() => import("./panels/Friends.route"));
const SmartWearablesPanel = lazy(() => import("./panels/SmartWearables.route"));
const GalleryPanel = lazy(() => import("./panels/Gallery.route"));
const LobbyHome = lazy(() => import("../explorer/pages/LobbyHome"));

const LINK_TO_ID: Record<string, string> = {
  "Explorer/Pages/Passport": "passport",
  "Explorer/Components/Notifications": "notifications",
  "Explorer/Pages/Friends": "friends",
  "Explorer/Pages/Backpack": "backpack",
  "Explorer/Pages/Reel": "gallery",
  "Explorer/Components/VoiceChat": "voicechat",
  "Explorer/Components/SmartWearables": "smartwearables",
  "Explorer/Components/SkyboxHUD": "skybox",
  "Explorer/Pages/Camera": "camera",
  "Explorer/Frames/Chat": "chat",
  "Explorer/Pages/ChatProfile": "passport",
  "Explorer/Pages/BackpackEmotes": "backpack",
  "Explorer/Pages/BadgesDetails": "passport?section=badges",
  "Explorer/Pages/SidebarHelp": "help?section=sidebar",
  "Explorer/Components/CommunityStream": "communities",
  "Explorer/Pages/Marketplace": "marketplace",
  "Explorer/Pages/Help": "help",
  "Explorer/Pages/FeatureFlags": "settings?section=flags",
};
for (const t of EXPLORE_TABS) {
  if (t.to) LINK_TO_ID[t.to] = t.id;
}

const HINT_TO_ID: Record<string, string> = {};
for (const t of EXPLORE_TABS) {
  if (t.hint) HINT_TO_ID[t.hint.toLowerCase()] = t.id;
}

const PANEL_IDS = new Set<string>(EXPLORE_TABS.map((t) => t.id));

function consumedOpenPanelNonce(): number {
  return (typeof window === "undefined" ? 0 : window.__dclConsumedOpenPanelNonce) ?? 0;
}

const ENGINE_JUMP_MIN_PARCEL_DELTA = 2;
const ENGINE_JUMP_MAX_MS = 30000;

function linkedId(target: EventTarget | null): string | null {
  const el = target instanceof Element ? target.closest("[data-sb-linkto]") : null;
  if (!el) return null;
  return LINK_TO_ID[el.getAttribute("data-sb-linkto") ?? ""] ?? null;
}

function isTextEntry(el: Element | null): boolean {
  if (!el) return false;
  const tag = el.tagName;
  return (
    tag === "INPUT" ||
    tag === "TEXTAREA" ||
    tag === "SELECT" ||
    (el instanceof HTMLElement && el.isContentEditable)
  );
}

function focusWorldCanvas(): boolean {
  if (typeof document === "undefined") return false;
  const c = document.getElementById(WORLD_CANVAS_ID);
  if (!c) return false;
  if (document.activeElement === c) return true;
  if (isTextEntry(document.activeElement)) return false;
  try {
    c.focus({ preventScroll: true });
  } catch {
  }
  return document.activeElement === c;
}

function MinimapWidget() {
  const scene = useBridgeState((s) => s.scene);
  const playerPosition = useBridgeState((s) => s.playerPosition);
  return (
    <div className="ui3-overlay__widget ui3-overlay__minimap">
      <Minimap
        place={scene?.title ?? undefined}
        coords={playerPosition?.parcel ?? scene?.coords ?? undefined}
        heading={playerPosition?.heading}
      />
    </div>
  );
}

type AppLayoutProps = {
  prefetchPanel?: (queryClient: QueryClient, id: string, address?: string | null) => void;
  prefetchAllPanels?: (queryClient: QueryClient, address?: string | null) => unknown;
};

export default function AppLayout({ prefetchPanel, prefetchAllPanels }: AppLayoutProps) {
  const navigate = useNavigate();
  const location = useLocation();
  const active = location.pathname.replace(/^\/+/, "").split("/")[0] || "";
  const worldPopup = active === "friends" || active === "skybox" || active === "smartwearables";
  const chatProfile = isChatProfile(location);
  const queryClient = useQueryClient();
  const identity = useBridgeState((s) => s.identity);
  const live = useBridgeState((s) => s.live);
  const avatarPreview = useBridgeState((s) => s.avatarPreview);
  const scene = useBridgeState((s) => s.scene);
  const connection = useBridgeState((s) => s.connection);
  const toasts = useBridgeState((s) => s.toasts);
  const openPanel = useBridgeState((s) => s.openPanel);
  const loading = useBridgeState((s) => s.loading);
  const parcel = useBridgeState((s) => s.playerPosition?.parcel ?? null);
  const [worldReadyOnce, setWorldReadyOnce] = useState(false);
  const [engineJumpArmed, setEngineJumpArmed] = useState(false);
  const transfers = useLoadingTransfers(engineJumpArmed);
  const prevRealmRef = useRef<string | null>(null);
  const prevParcelRef = useRef<string | null>(null);
  const { unread: sidebarUnread } = useNotifications();
  const emotes = useOwnedEmotes(identity.address);
  const isMobile = useIsMobile();
  const [sidebarDesignFlag] = useState(sidebarDesignEnabled);
  const entry = useWorldEntry();
  const [lobbyReady, setLobbyReady] = useState(false);
  const onLobbyReady = useCallback(() => setLobbyReady(true), []);
  const warmedFor = useRef<string | null | undefined>(undefined);
  const startupReady = lobbyReady || Boolean(loading?.avatarLoaded && loading.ready);
  useEffect(() => {
    const viewer = identity.address ?? null;
    if (!startupReady || !prefetchAllPanels || warmedFor.current === viewer) return;
    const timer = setTimeout(() => {
      warmedFor.current = viewer;
      void prefetchAllPanels(queryClient, viewer);
    }, 0);
    return () => clearTimeout(timer);
  }, [startupReady, prefetchAllPanels, queryClient, identity.address]);
  const [lobbyOpen, setLobbyOpen] = useState(() => entry?.pending ?? true);
  const enterWorld = useCallback(() => {
    if (entry?.pending) { entry.enter(null); return; }
    setLobbyOpen(false);
    navigate("/");
  }, [navigate, entry]);
  const sidebarDesign = sidebarDesignFlag && !isMobile;
  const SidebarView = sidebarDesign ? SidebarDesign : Sidebar;
  const [sidebarDrawer, setSidebarDrawer] = useState<SidebarDrawer | null>(null);
  const orientation = useViewportOrientation();
  const [profileOpen, setProfileOpen] = useState(false);
  const [chatOpen, setChatOpen] = useState(false);
  const [emoteOpen, setEmoteOpen] = useState(false);
  const [connectionOpen, setConnectionOpen] = useState(false);
  const [connectionStatusEnabled] = useState(() => flagState("2026-09-connection-status").enabled);
  const [leftPanel, setLeftPanel] = useState<LeftPanelId | null>(null);
  const toggleLeft = useCallback(
    (id: LeftPanelId) => {
      if (worldPopup) navigate("/");
      setSidebarDrawer(null);
      setChatOpen(false);
      setProfileOpen(false);
      setEmoteOpen(false);
      setLeftPanel((p) => (p === id ? null : id));
    },
    [navigate, worldPopup],
  );
  const closeOverlays = useCallback(() => {
    setLeftPanel(null);
  }, []);
  const onProfileToggle = useCallback(() => {
    if (worldPopup) navigate("/");
    setSidebarDrawer(null);
    setLeftPanel(null);
    setChatOpen(false);
    setEmoteOpen(false);
    setProfileOpen((o) => !o);
  }, [navigate, worldPopup]);
  const openLobby = useCallback(() => {
    setLobbyOpen(true);
    setLeftPanel(null);
    setChatOpen(false);
    setProfileOpen(false);
    setEmoteOpen(false);
    setSidebarDrawer(null);
    setConnectionOpen(false);
    navigate("/");
  }, [navigate]);
  const onChatToggle = useCallback(() => {
    if (worldPopup) navigate("/");
    setSidebarDrawer(null);
    setLeftPanel(null);
    setProfileOpen(false);
    setEmoteOpen(false);
    setChatOpen((o) => !o);
  }, [navigate, worldPopup]);
  const onEmoteToggle = useCallback(() => {
    if (worldPopup) navigate("/");
    setSidebarDrawer(null);
    setLeftPanel(null);
    setProfileOpen(false);
    setChatOpen(false);
    setEmoteOpen((o) => !o);
  }, [navigate, worldPopup]);
  const onSignOut = useCallback(() => {
    setProfileOpen(false);
    sendBridge("Logout", {});
    signOutEngineAuth();
    window.setTimeout(() => window.location.assign(window.location.pathname), 200);
  }, []);

  const onDrawerChange = useCallback((drawer: SidebarDrawer | null, dismissPopups = false) => {
    setSidebarDrawer(drawer);
    if (!drawer && !dismissPopups) return;
    if (worldPopup) navigate("/");
    setLeftPanel(null);
    setProfileOpen(false);
    setChatOpen(false);
    setEmoteOpen(false);
    setConnectionOpen(false);
  }, [navigate, worldPopup]);

  const chatProfileOpener = useRef<HTMLElement | null>(null);
  const viewChatProfile = useCallback<ViewChatProfile>((address, opener) => {
    chatProfileOpener.current = opener instanceof HTMLElement ? opener : null;
    navigate(chatProfilePath(address), { state: CHAT_PROFILE_STATE });
  }, [navigate]);
  useEffect(() => {
    if (chatProfile) return;
    const opener = chatProfileOpener.current;
    chatProfileOpener.current = null;
    if (opener?.isConnected) opener.focus({ preventScroll: true });
  }, [chatProfile]);

  const notifOpen = leftPanel === "notif";
  const voiceOpen = leftPanel === "voice";
  const skyboxOpen = leftPanel === "skybox";
  const portablesOpen = leftPanel === "portables";
  const friendsOpen = leftPanel === "friends";

  const showingLobby = lobbyOpen && active === "";
  const lobbySettings = lobbyOpen && active === "settings";
  useEffect(() => {
    if (active !== "settings" || lobbyOpen || entry?.pending) return;
    const resume = (event: PointerEvent) => {
      if (!(event.target instanceof Element) || event.target.id !== "mygame-canvas") return;
      setLobbyOpen(false);
      navigate("/");
      sendBridge("SetExplorerUiOpen", { ui: null });
      const canvas = event.target as HTMLCanvasElement;
      canvas.focus();
      void canvas.requestPointerLock?.()?.catch(() => {});
    };
    document.addEventListener("pointerdown", resume, true);
    return () => document.removeEventListener("pointerdown", resume, true);
  }, [active, navigate, lobbyOpen, entry?.pending]);
  const user = identity.name || "Guest";

  useEffect(() => {
    if (active !== "") setSidebarDrawer(null);
    if (active === "") stopEmote();
    else {
      closeOverlays();
      setProfileOpen(false);
      if (!chatProfile) setChatOpen(false);
      setEmoteOpen(false);
    }
  }, [active, closeOverlays, chatProfile]);

  useEffect(() => {
    if (active !== "" || showingLobby) return undefined;
    let tries = 0;
    let t: ReturnType<typeof setTimeout> | undefined;
    const tick = () => {
      if (typeof document !== "undefined" && document.querySelector(".xc")) return;
      if (focusWorldCanvas() || tries++ > 30) return;
      t = setTimeout(tick, 100);
    };
    tick();
    return () => {
      if (t) clearTimeout(t);
    };
  }, [active, showingLobby]);

  const onPointerUp = useCallback((e: ReactPointerEvent) => {
    const t = e.target;
    if (!(t instanceof Element)) return;
    if (!t.closest(".ui3-overlay")) return;
    if (t.closest('.sd, [data-sb-panel="profile"]')) return;
    if (isTextEntry(t) || t.closest("input, textarea, select")) return;
    setTimeout(() => {
      if (document.activeElement?.closest('[role="menu"]')) return;
      focusWorldCanvas();
    }, 0);
  }, []);

  useEffect(() => {
    sendBridge("RequestAvatarPreview", {});
  }, []);

  useEffect(() => {
    if (!openPanel || openPanel.nonce <= consumedOpenPanelNonce()) return;
    window.__dclConsumedOpenPanelNonce = openPanel.nonce;
    if (PANEL_IDS.has(openPanel.ui)) navigate(`/${openPanel.ui}`);
  }, [openPanel, navigate]);

  useEffect(() => {
    sendBridge("SetExplorerUiOpen", { ui: showingLobby || lobbySettings ? "lobby" : worldPopup ? null : active || null });
  }, [active, showingLobby, lobbySettings, worldPopup]);

  const [engineJumpStalled, setEngineJumpStalled] = useState(false);
  const dismissEngineJump = useCallback(() => {
    setEngineJumpArmed(false);
    setEngineJumpStalled(false);
  }, []);

  useEffect(() => {
    if (loading?.ready) {
      setWorldReadyOnce(true);
      setEngineJumpArmed(false);
      setEngineJumpStalled(false);
    }
  }, [loading?.ready]);

  useEffect(() => {
    const prev = prevRealmRef.current;
    prevRealmRef.current = scene.realm;
    if (worldReadyOnce && prev != null && scene.realm != null && scene.realm !== prev)
      setEngineJumpArmed(true);
  }, [scene.realm, worldReadyOnce]);

  useEffect(() => {
    const prev = prevParcelRef.current;
    prevParcelRef.current = parcel;
    if (!worldReadyOnce || prev == null || parcel == null || parcel === prev) return;
    const [px = NaN, py = NaN] = prev.split(",").map(Number);
    const [nx = NaN, ny = NaN] = parcel.split(",").map(Number);
    if (Math.max(Math.abs(nx - px), Math.abs(ny - py)) > ENGINE_JUMP_MIN_PARCEL_DELTA)
      setEngineJumpArmed(true);
  }, [parcel, worldReadyOnce]);

  useEffect(() => {
    if (!engineJumpArmed) {
      setEngineJumpStalled(false);
      return undefined;
    }
    setEngineJumpStalled(false);
    const t = window.setTimeout(() => setEngineJumpStalled(true), ENGINE_JUMP_MAX_MS);
    return () => window.clearTimeout(t);
  }, [engineJumpArmed, loading?.percent, loading?.pendingAssets, transfers.receivedBytes, transfers.completed]);

  const panelJumpActive = usePanelJumpActive();
  const engineTeleporting =
    engineJumpArmed && loading != null && !loading.ready && !panelJumpActive;

  const onTab = useCallback(
    (id: string) => navigate(id === active ? "/" : `/${id}`),
    [navigate, active],
  );

  const onIntent = useCallback(
    (e: SyntheticEvent) => {
      const intent = e.target instanceof Element ? e.target.closest("[data-panel-preload]") : null;
      const id = intent?.getAttribute("data-panel-preload") ?? linkedId(e.target);
      if (id && prefetchPanel) prefetchPanel(queryClient, id, identity.address);
    },
    [prefetchPanel, queryClient, identity.address],
  );

  const onClickCapture = useCallback(
    (e: ReactMouseEvent) => {
      const id = linkedId(e.target);
      if (id) navigate(`/${id}`);
    },
    [navigate],
  );

  useEffect(() => {
    const onKey = (e: KeyboardEvent) => {
      if (isSynthesizedWorldKey(e)) return;
      if (e.metaKey || e.ctrlKey || e.altKey) return;
      if (e.key === "Escape" && e.repeat) return;
      if (e.key === "Escape" && document.querySelector(".lh__header-menu, .ps__name-form, [role=alertdialog], [role=menu]")) return;
      if (showingLobby) {
        if (e.key === "Escape" && !entry?.pending && !panelJumpActive && !engineTeleporting) {
          setLobbyOpen(false);
          focusWorldCanvas();
          e.preventDefault();
          e.stopImmediatePropagation();
        }
        return;
      }
      if (e.key === "Escape") {
        if (panelJumpActive || engineTeleporting) return;
        if (sidebarDrawer) {
          setSidebarDrawer(null);
          document.querySelector<HTMLButtonElement>('.sd__dock [aria-expanded="true"]')?.focus();
          e.preventDefault();
          e.stopImmediatePropagation();
          return;
        }
        if (active) {
          navigate("/");
          if (!lobbyOpen && !chatProfile) focusWorldCanvas();
          e.preventDefault();
          e.stopImmediatePropagation();
        } else if (leftPanel || profileOpen || chatOpen || emoteOpen || connectionOpen) {
          closeOverlays();
          setProfileOpen(false);
          setChatOpen(false);
          setEmoteOpen(false);
          setConnectionOpen(false);
          focusWorldCanvas();
          e.preventDefault();
          e.stopImmediatePropagation();
        } else {
          openLobby();
          e.preventDefault();
          e.stopImmediatePropagation();
        }
        return;
      }
      const ae = document.activeElement;
      if (e.key === "Enter" && ae?.closest("button, a, select")) return;
      if (
        ae &&
        (ae.tagName === "INPUT" ||
          ae.tagName === "TEXTAREA" ||
          (ae instanceof HTMLElement && ae.isContentEditable))
      )
        return;
      const menuKey = e.key.toLowerCase();
      const menuTarget = sidebarDrawer === "me" ? ({ p: "passport", b: "passport?section=badges" } as Record<string, string>)[menuKey]
        : sidebarDrawer === "system" && menuKey === "h" ? "help" : undefined;
      if (menuTarget) {
        navigate(`/${menuTarget}`);
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      if (e.key.toLowerCase() === "b" && !active) {
        onEmoteToggle();
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      if (e.key === "Enter" && !active && !chatOpen) {
        onChatToggle();
        e.preventDefault();
        e.stopImmediatePropagation();
        return;
      }
      if (active === "camera") return;
      const id = HINT_TO_ID[e.key.toLowerCase()];
      if (id) {
        navigate(id === active ? "/" : `/${id}`);
        e.preventDefault();
        e.stopImmediatePropagation();
      }
    };
    window.addEventListener("keydown", onKey, true);
    return () => window.removeEventListener("keydown", onKey, true);
  }, [
    active,
    navigate,
    leftPanel,
    profileOpen,
    chatOpen,
    emoteOpen,
    closeOverlays,
    panelJumpActive,
    engineTeleporting,
    sidebarDrawer,
    connectionOpen,
    sidebarDesign,
    showingLobby,
    entry?.pending,
    lobbyOpen,
    openLobby,
    onChatToggle,
    onEmoteToggle,
    chatProfile,
  ]);

  useEffect(() => {
    const root = document.documentElement;
    if (isMobile) root.setAttribute("data-mobile-chrome", "");
    else root.removeAttribute("data-mobile-chrome");
    return () => root.removeAttribute("data-mobile-chrome");
  }, [isMobile]);

  return (
    <AudioMixerProvider>
    <JumpCompleteContext value={enterWorld}>
    <GraphicsProgress />
    <OrientationProvider>
      <div
        className="ui3-app-root"
        data-mobile={isMobile ? "true" : "false"}
        onMouseOverCapture={onIntent}
        onFocusCapture={onIntent}
        onPointerDownCapture={onIntent}
        onClickCapture={onClickCapture}
        onPointerUp={onPointerUp}
      >
        {lobbySettings && <div className="explorer-lobby-backdrop" inert><Suspense fallback={null}><LobbyHome onEnterWorld={enterWorld} onSignOut={onSignOut} /></Suspense></div>}
        {showingLobby ? <Suspense fallback={<ShellLoading label="Opening your lobby&#x2026;" />}><LobbyHome onEnterWorld={enterWorld} onSignOut={onSignOut} onReady={onLobbyReady} /></Suspense> : active === "" || worldPopup || chatProfile ? (
          <>
            <MinimapVisibilityProvider initiallyVisible={sidebarDesign}>
            <div
              className="ui3-overlay"
              data-live={live ? "true" : "false"}
              data-sidebar-design={sidebarDesign ? SIDEBAR_DESIGN_FLAG : undefined}
            >
              <div className="ui3-overlay__widget ui3-overlay__sidebar">
                <SidebarView
                  drawer={sidebarDrawer}
                  onDrawerChange={onDrawerChange}
                  onConnectionToggle={connectionStatusEnabled ? () => setConnectionOpen((o) => !o) : undefined}
                  onSignOut={onSignOut}
                  avatarPreview={avatarPreview}
                  onProfileToggle={onProfileToggle}
                  onLobbyOpen={openLobby}
                  profileOpen={profileOpen}
                  chatOpen={chatOpen}
                  onChatToggle={onChatToggle}
                  notifOpen={notifOpen}
                  onNotifToggle={() => toggleLeft("notif")}
                  voiceOpen={voiceOpen}
                  onVoiceToggle={() => toggleLeft("voice")}
                  skyboxOpen={skyboxOpen}
                  onSkyboxToggle={() => toggleLeft("skybox")}
                  portablesOpen={portablesOpen}
                  onPortablesToggle={() => toggleLeft("portables")}
                  friendsOpen={friendsOpen}
                  onFriendsToggle={() => toggleLeft("friends")}
                  emoteOpen={emoteOpen}
                  onEmoteToggle={onEmoteToggle}
                  unread={sidebarUnread}
                />
              </div>
              {!sidebarDesign && !sidebarDrawer && !leftPanel && !chatOpen && !profileOpen && !emoteOpen && <MinimapWidget />}
              <div className="ui3-overlay__widget ui3-overlay__profile">
                <ProfileWidget
                  open={profileOpen}
                  name={identity.name}
                  tag={identity.tag ?? undefined}
                  wallet={identity.wallet ?? undefined}
                  address={identity.address ?? undefined}
                  avatarSrc={avatarPreview}
                  isGuest={identity.isGuest}
                  onClose={onProfileToggle}
                  onSignOut={onSignOut}
                />
              </div>
              <div className="ui3-overlay__widget ui3-overlay__chat">
                <ChatProfileContext value={viewChatProfile}><Chat open={chatOpen} onToggle={onChatToggle} hidden={leftPanel != null} /></ChatProfileContext>
              </div>
              {notifOpen && (
                <div className="ui3-overlay__widget ui3-overlay__notifications">
                  <FloatingPanel id="notifications" onClose={closeOverlays} flush>
                    <PanelBoundary label="notifications" onClose={closeOverlays}>
                      <NotificationsPanel floating />
                    </PanelBoundary>
                  </FloatingPanel>
                </div>
              )}
              {voiceOpen && (
                <div className="ui3-overlay__widget ui3-overlay__voice">
                  <FloatingPanel id="voice" onClose={closeOverlays}>
                    <VoiceControls />
                  </FloatingPanel>
                </div>
              )}
              {skyboxOpen && (
                <div className="ui3-overlay__widget ui3-overlay__skybox">
                  <FloatingPanel id="skybox" onClose={closeOverlays}>
                    <SkyboxControls />
                  </FloatingPanel>
                </div>
              )}
              {portablesOpen && (
                <div className="ui3-overlay__widget ui3-overlay__portables">
                  <FloatingPanel id="portables" onClose={closeOverlays}>
                    <PanelBoundary label="portable experiences" onClose={closeOverlays}>
                      <SmartWearablesPanel floating />
                    </PanelBoundary>
                  </FloatingPanel>
                </div>
              )}
              {friendsOpen && (
                <div className="ui3-overlay__widget ui3-overlay__friends">
                  <FloatingPanel id="friends" onClose={closeOverlays} flush>
                    <PanelBoundary label="friends" onClose={closeOverlays}>
                      <FriendsPanel floating />
                    </PanelBoundary>
                  </FloatingPanel>
                </div>
              )}
              {emoteOpen && (
                <div className="ui3-overlay__widget ui3-overlay__emote">
                  <EmoteWheel
                    catalog={emotes.data?.catalog ?? []}
                    loading={emotes.isPending}
                    error={emotes.isError}
                    onRetry={() => void emotes.refetch()}
                    onCustomise={() => { setEmoteOpen(false); navigate("/backpack"); }}
                    loadout={emotes.data?.loadout ?? []}
                    onSelect={() => setEmoteOpen(false)}
                    onClose={() => setEmoteOpen(false)}
                  />
                </div>
              )}
              {connectionStatusEnabled && (() => {
                const c = connection;
                const health =
                  c == null
                    ? "info"
                    : c.globalRoom && c.sceneHealth === "ok"
                      ? "ok"
                      : "warn";
                return (
                  <div className="ui3-overlay__widget ui3-overlay__connbadge">
                    <button
                      type="button"
                      className={"connbadge connbadge--" + health}
                      aria-label="Connection status"
                      aria-expanded={connectionOpen}
                      title="Connection status"
                      onClick={() => setConnectionOpen((o) => !o)}
                    >
                      <span className="connbadge__dot" />
                    </button>
                  </div>
                );
              })()}
              {connectionStatusEnabled && connectionOpen && (
                <div className="ui3-overlay__widget ui3-overlay__connection">
                  <ConnectionStatus
                    connection={connection}
                    realm={scene.realm ?? undefined}
                    onClose={() => setConnectionOpen(false)}
                  />
                </div>
              )}
              {worldPopup && (
                <FloatingPanel id={(active === "smartwearables" ? "portables" : active) as FloatingPanelId} onClose={() => navigate("/")} flush={active === "friends"}>
                  <PanelBoundary key={active} label={active === "smartwearables" ? "portable experiences" : active === "skybox" ? "time of day" : active} onClose={() => navigate("/")}>
                    {active === "friends" ? <FriendsPanel floating /> : active === "skybox" ? <SkyboxControls /> : active === "smartwearables" ? <SmartWearablesPanel floating /> : <GalleryPanel />}
                  </PanelBoundary>
                </FloatingPanel>
              )}
              <EngineToasts toasts={toasts} />
            </div>
            </MinimapVisibilityProvider>
            {chatProfile ? <PanelBoundary key="passport" label="this profile" onClose={() => navigate("/")} standalone><Outlet /></PanelBoundary> : !worldPopup && <Outlet />}
          </>
        ) : active === "camera" || active === "passport" ? <PanelBoundary key={active} label={active === "passport" ? "your profile" : "camera"} onClose={() => navigate("/")} standalone><Outlet /></PanelBoundary> : active === "settings" ? (
          <section className="explorer-settings-panel" role="region" aria-label="Explorer settings">
            <PanelBoundary label="settings" onClose={() => navigate("/")}><Outlet /></PanelBoundary>
          </section>
        ) : (
          <ExploreChrome
            active={active as TabId}
            onTab={onTab}
            user={user}
            onClose={() => navigate("/")}
            avatarSrc={avatarPreview}
            tag={identity.tag ?? undefined}
            wallet={identity.wallet ?? undefined}
            address={identity.address ?? undefined}
            isGuest={identity.isGuest}
            profileOpen={profileOpen}
            onProfileToggle={onProfileToggle}
            onLobbyOpen={openLobby}
            onSignOut={onSignOut}
          >
            <PanelBoundary key={active} label={EXPLORE_TABS.find(tab => tab.id === active)?.label.toLowerCase() ?? "this panel"} onClose={() => navigate("/")}>
              {active === "gallery" ? <GalleryPanel embedded={false} /> : <Outlet />}
            </PanelBoundary>
          </ExploreChrome>
        )}
        {isMobile && !showingLobby && (
          <Suspense fallback={null}>
            <MobileHudFrame
              orientation={orientation}
              place={scene.title ?? ""}
              coords={scene.coords ?? ""}
              unread={sidebarUnread}
              crosshair={active === ""}
              activeTab={active}
              onTab={onTab}
              onMenu={() => navigate("/places")}
              onChat={() => setChatOpen((o) => !o)}
              onProfile={openLobby}
              avatarSrc={avatarPreview}
              user={user}
              controlsSlot={
                active === "" ? (
                  <TouchControls
                    canvasId={WORLD_CANVAS_ID}
                    enabled={leftPanel == null && !profileOpen && !chatOpen && !emoteOpen}
                  />
                ) : null
              }
            />
          </Suspense>
        )}
        {engineTeleporting && (
          <JumpLoading
            name={scene.title ?? undefined}
            stalled={engineJumpStalled}
            onCancel={dismissEngineJump}
            onEnterAnyway={dismissEngineJump}
          />
        )}
        <LoginCodeModal />
        <PermissionPrompt />
      </div>
    </OrientationProvider>
    </JumpCompleteContext>
    </AudioMixerProvider>
  );
}
