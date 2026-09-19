import { useEffect, useRef } from "react";
import { useNavigate } from "react-router";
import NotificationsPanel from "../../../app/panels/Notifications.route";
import { useBridgeState } from "../../../overlay/bridge";
import Icon from "../../frames/SidebarDesignIcon";

export default function LobbyHeaderMenu({ kind, onClose, onSignOut }: {
  kind: "account" | "notifications"; onClose: () => void; onSignOut?: () => void;
}) {
  const ref = useRef<HTMLDivElement>(null);
  const closeRef = useRef(onClose);
  closeRef.current = onClose;
  const navigate = useNavigate();
  const identity = useBridgeState(s => s.identity);
  useEffect(() => {
    const trigger = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    ref.current?.querySelector<HTMLButtonElement>("button")?.focus();
    const outside = (e: PointerEvent) => {
      if (e.target instanceof Element && !e.target.closest('.lh__header')) closeRef.current();
    };
    const escape = (e: KeyboardEvent) => {
      if (e.key !== "Escape") return;
      e.preventDefault(); e.stopImmediatePropagation(); closeRef.current(); trigger?.focus();
    };
    document.addEventListener("pointerdown", outside);
    window.addEventListener("keydown", escape, true);
    return () => { document.removeEventListener("pointerdown", outside); window.removeEventListener("keydown", escape, true); };
  }, [kind]);
  const go = (path: string) => { onClose(); navigate(path); };
  return <div ref={ref} className="lh__header-menu" role="dialog" aria-label={kind === "account" ? "Your account" : "Notifications"}>
    <button className="lh__menu-close" aria-label="Close menu" onClick={onClose}><Icon name="close" /></button>
    {kind === "notifications" ? <NotificationsPanel floating /> : <>
      <strong>{identity.name || "Guest"}</strong>
      <button onClick={() => go("/passport")}><Icon name="user" />Profile</button>
      <button onClick={() => go("/backpack")}><Icon name="pack" />Edit avatar</button>
      <button onClick={() => go("/settings")}><Icon name="settings" />Settings</button>
      <button onClick={onSignOut}><Icon name="power" />{identity.isGuest ? "Sign in" : "Sign out"}</button>
    </>}
  </div>;
}
