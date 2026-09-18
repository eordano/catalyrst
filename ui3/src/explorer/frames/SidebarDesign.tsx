import { useEffect, useRef, useState, type MouseEvent } from "react";
import { Avatar } from "../../atoms/primitives";
import Tooltip from "../../atoms/Tooltip";
import { useBridgeState } from "../../overlay/bridge";
import { isNativeHost } from "../../overlay/nativeHost";
import { siteUrl } from "../../data/site";
import { useSidebarAnchor } from "../components/FloatingPanel";
import type { SidebarProps } from "./Sidebar";
import Icon, { type SidebarDesignIconName } from "./SidebarDesignIcon";
import QuickAudio from "../components/QuickAudio";
import SidebarSceneCard from "./SidebarSceneCard";
import WalletAddress from "../components/WalletAddress";
import "./sidebardesign.css";

export type SidebarDrawer = "me" | "location" | "discover" | "system" | "audio";
type Props = SidebarProps & {
  drawer: SidebarDrawer | null;
  onDrawerChange: (drawer: SidebarDrawer | null, dismissPopups?: boolean) => void;
  onConnectionToggle?: () => void;
  onSignOut?: () => void;
};

function Action({ icon, label, to, href, onClick, danger }: {
  icon: SidebarDesignIconName; label: string; to?: string; href?: string;
  onClick?: () => void; shortcut?: string; danger?: boolean;
}) {
  const content = <><Icon name={icon} /><span>{label}</span></>;
  return href
    ? <a className="sd__action" href={href} target="_blank" rel="noopener noreferrer">{content}</a>
    : <button type="button" className="sd__action" aria-label={label} data-danger={danger || undefined} data-sb-linkto={to} onClick={onClick}>{content}</button>;
}

export default function SidebarDesign(props: Props) {
  const { drawer, onDrawerChange, unread = 0 } = props;
  const identity = useBridgeState((s) => s.identity);
  const friends = useBridgeState((s) => s.friends);
  const portables = useBridgeState(s => s.portables);
  const mic = useBridgeState((s) => s.mic);
  const panel = useRef<HTMLElement>(null);
  const dock = useRef<HTMLElement>(null);
  const lastTrigger = useRef<HTMLButtonElement | null>(null);
  const [locationPinned, setLocationPinned] = useState(() => {
    try { return localStorage.getItem("dcl.location.pinned") === "1"; } catch { return false; }
  });
  const toggleLocation = () => {
    onDrawerChange(null, true);
    const next = !locationPinned;
    setLocationPinned(next);
    try { localStorage.setItem("dcl.location.pinned", next ? "1" : "0"); } catch {}
  };
  useSidebarAnchor("design-" + drawer, panel, !!drawer);
  useEffect(() => {
    if (!drawer) return;
    panel.current?.querySelector<HTMLElement>("button:not(:disabled), input:not(:disabled)")?.focus();
    const outside = (event: PointerEvent) => {
      if (!(event.target instanceof Node) || panel.current?.contains(event.target) || dock.current?.contains(event.target) || (event.target instanceof Element && event.target.closest('[role="dialog"]'))) return;
      onDrawerChange(null);
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [drawer, onDrawerChange]);
  const close = () => { onDrawerChange(null); lastTrigger.current?.focus(); };
  const launch = (action?: () => void) => () => { onDrawerChange(null); action?.(); };
  const trigger = (id: SidebarDrawer) => (event: MouseEvent<HTMLButtonElement>) => {
    lastTrigger.current = event.currentTarget;
    onDrawerChange(drawer === id ? null : id);
  };
  const circle = (label: string, icon: SidebarDesignIconName, options: {
    onClick?: () => void; to?: string; anchor?: string; active?: boolean; dot?: boolean;
  } = {}) => <Tooltip label={label} side="right" portal>
    <button type="button" className="sd__circle" aria-label={label} aria-expanded={options.onClick ? !!options.active : undefined}
      data-selected={options.active || undefined} data-sb-panel={options.anchor} data-sb-linkto={options.to} onClick={options.onClick}>
      <Icon name={icon} button />{options.dot && <i className="sd__presence" />}
    </button>
  </Tooltip>;
  const titles: Record<SidebarDrawer, string> = { me: "ME", location: "LOCATION", discover: "Discover", system: "System", audio: "Audio" };
  const avatar = (size: number) => <Avatar size={size} name={identity.name || "Guest"} src={props.avatarPreview || undefined} seed={identity.address || "guest"} />;
  return <div className="sd">
    <nav ref={dock} className="sd__dock" aria-label="Main menu">
      <div className="sd__main">
        <Tooltip label="Me" side="right" portal><button type="button" className="sd__circle sd__profile" aria-label="Me" data-sb-panel="design-me" data-selected={drawer === "me" || undefined} aria-expanded={drawer === "me"} aria-controls={drawer === "me" ? "sidebar-design-drawer" : undefined} onClick={trigger("me")}>{avatar(38)}{unread > 0 && <i className="sd__unread" aria-label="Unread notifications" />}</button></Tooltip>
        <Tooltip label="Location" side="right" portal><button type="button" className="sd__circle" aria-label="Location" data-sb-panel="design-location" data-selected={locationPinned || undefined} aria-pressed={locationPinned} aria-expanded={locationPinned} aria-controls="sidebar-location" onClick={toggleLocation}><Icon name="pin" /></button></Tooltip>
        <Tooltip label="Discover" side="right" portal><button type="button" className="sd__circle" aria-label="Discover" data-sb-panel="design-discover" data-selected={drawer === "discover" || undefined} aria-expanded={drawer === "discover"} onClick={trigger("discover")}><Icon name="compass" button /></button></Tooltip>
        <Tooltip label="System" side="right" portal><button type="button" className="sd__circle" aria-label="System" data-sb-panel="design-system" data-selected={drawer === "system" || undefined} aria-expanded={drawer === "system"} aria-controls={drawer === "system" ? "sidebar-design-drawer" : undefined} onClick={trigger("system")}><Icon name="settings" button /></button></Tooltip>
      </div>
      <div className="sd__quick" aria-label="Quick actions">
        {portables.length > 0 && circle("Portable experiences", "bolt", { anchor: "portables", onClick: launch(props.onPortablesToggle), active: props.portablesOpen })}
        {circle("Camera", "camera", { to: "Explorer/Pages/Camera" })}
        {friends.friends.length > 0 && circle("Friends", "friends", { anchor: "friends", onClick: launch(props.onFriendsToggle), active: props.friendsOpen, dot: friends.onlineCount > 0 })}
        {circle("Emotes", "emote", { onClick: launch(props.onEmoteToggle), active: props.emoteOpen })}
        {circle(mic.enabled ? "Voice Chat, microphone on" : "Voice Chat", "voice", { anchor: "voice", onClick: launch(props.onVoiceToggle), active: props.voiceOpen, dot: mic.enabled })}
        <Tooltip label="Audio" side="right" portal><button className="sd__circle" aria-label="Audio" data-sb-panel="design-audio" aria-expanded={drawer === "audio"} onClick={trigger("audio")}><Icon name="audio" /></button></Tooltip>
        {circle("Chat", "chat", { anchor: "chat", onClick: launch(props.onChatToggle), active: props.chatOpen })}
      </div>
    </nav>
    {locationPinned && <section id="sidebar-location" className="sd__drawer sd__pinned-location" data-drawer="location" aria-label="Location"><SidebarSceneCard onSkybox={launch(props.onSkyboxToggle)} onNearby={launch(props.onChatToggle)} /></section>}
    {drawer && <section ref={panel} id="sidebar-design-drawer" className="sd__drawer" data-drawer={drawer} aria-label={titles[drawer]}>
      {!["me", "location"].includes(drawer) && <header className="sd__heading"><h2>{titles[drawer]}</h2><button type="button" aria-label="Close menu" onClick={close}><Icon name="close" /></button></header>}
      {drawer === "audio" && <QuickAudio />}
      {drawer === "location" && <SidebarSceneCard onSkybox={launch(props.onSkyboxToggle)} onNearby={launch(props.onChatToggle)} />}
      {drawer === "me" && <>
        <div className="sd__identity"><button type="button" className="sd__avatar-link" aria-label="Open profile" data-sb-linkto="Explorer/Pages/Passport">{avatar(52)}</button><div><div className="sd__name"><strong>{identity.name || "Guest"}</strong>{identity.tag && <span>{identity.tag.startsWith("#") ? identity.tag : "#" + identity.tag}</span>}</div>{identity.address ? <WalletAddress address={identity.address} compact /> : <small>Guest</small>}</div></div>
        <div className="sd__actions"><Action icon="home" label="Home" onClick={launch(props.onLobbyOpen)} /><Action icon="pack" label="Backpack" to="Explorer/Pages/Backpack" shortcut="I" /><Action icon="user" label="Profile" to="Explorer/Pages/Passport" shortcut="P" /><Action icon="badge" label="Badges" to="Explorer/Pages/BadgesDetails" shortcut="B" /><Action icon="gallery" label="Gallery" to="Explorer/Pages/Reel" shortcut="K" /><Action icon="bell" label={unread ? `Notifications (${unread} unread)` : "Notifications"} onClick={launch(props.onNotifToggle)} /></div>
        <div className="sd__actions sd__session"><Action icon="power" label={identity.isGuest ? "Sign In" : "Sign Out"} onClick={launch(identity.isGuest ? props.onProfileToggle : props.onSignOut)} />{isNativeHost() && <Action icon="exit" label="Exit" danger onClick={launch(props.onLobbyOpen ?? (() => window.location.assign(siteUrl())))} />}</div>
      </>}
      {drawer === "discover" && <div className="sd__actions"><Action icon="calendar" label="Events" to="Explorer/Pages/Events" shortcut="X" /><Action icon="pin" label="Places" to="Explorer/Pages/Places" shortcut="Z" /><Action icon="people" label="Communities" to="Explorer/Pages/Communities" shortcut="O" /><Action icon="bag" label="Marketplace" to="Explorer/Pages/Marketplace" /></div>}
      {drawer === "system" && <>
        <div className="sd__actions"><Action icon="sliders" label="Settings" to="Explorer/Pages/Settings" shortcut="P" /><Action icon="keyboard" label="Mouse / Key Controls" to="Explorer/Pages/Help" shortcut="H" /><Action icon="help" label="Sidebar guide" to="Explorer/Pages/SidebarHelp" /></div>
        <div className="sd__actions sd__separated"><Action icon="help" label="FAQ" href={siteUrl("/docs/")} /><Action icon="support" label="Report a bug / Contact support" href={siteUrl("/support")} /><Action icon="discord" label="Discord" href="https://decentraland.org/discord" /></div>
      </>}

    </section>}
  </div>;
}
