import { useEffect, useRef, useState } from "react";
import { useQueryClient } from "@tanstack/react-query";
import { useBridgeState } from "../../overlay/bridge";
import { usePlaces } from "../../data/hooks/usePlaces";
import { setPlaceFavorite } from "../../data/catalyst/places";
import { useMinimapVisibility } from "../../overlay/minimapVisibility";
import ContextMenu from "../../components/ContextMenu";
import Minimap, { jumpUrl } from "./Minimap";
import { SceneFeedbackModal, SceneTipModal, useSceneOwner } from "../components/SceneOwnerActions";
import { CatalystError } from "../../data/catalyst/client";
import Icon from "./SidebarDesignIcon";

export default function SidebarSceneCard({ onSkybox, onNearby }: { onSkybox: () => void; onNearby?: () => void }) {
  const scene = useBridgeState(s => s.scene);
  const position = useBridgeState(s => s.playerPosition);
  const players = useBridgeState(s => s.players);
  const identity = useBridgeState(s => s.identity);
  const { userHidden, toggleUserHidden } = useMinimapVisibility();
  const coords = position?.parcel || scene.coords || "";
  const client = useQueryClient();
  const places = usePlaces({ positions: coords, limit: 1 }, !!coords, !identity.isGuest && !!identity.address);
  const place = places.data?.[0];
  const [menu, setMenu] = useState(false);
  const [ownerModal, setOwnerModal] = useState<"feedback" | "tip" | null>(null);
  const owner = useSceneOwner(coords, scene.realm, menu || ownerModal !== null);
  const [favorite, setFavorite] = useState<{ id: string; value: boolean } | null>(null);
  const [saving, setSaving] = useState(false);
  const [notice, setNotice] = useState("");
  const [copied, setCopied] = useState(false);
  const copyTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(copyTimer.current), []);
  const ref = useRef<HTMLDivElement>(null);
  const favorited = favorite?.id === place?.id ? favorite?.value : place?.userFavorite === true;
  useEffect(() => {
    if (!menu) return;
    const outside = (event: PointerEvent) => {
      if (event.target instanceof Node && !ref.current?.contains(event.target)) setMenu(false);
    };
    document.addEventListener("pointerdown", outside, true);
    return () => document.removeEventListener("pointerdown", outside, true);
  }, [menu]);
  const copy = async (text: string) => {
    setMenu(false);
    try {
      await navigator.clipboard.writeText(text);
      setNotice("");
      setCopied(true);
      clearTimeout(copyTimer.current);
      copyTimer.current = setTimeout(() => setCopied(false), 1000);
    }
    catch { setNotice("Could not copy. Try again."); }
  };
  const toggleFavorite = async () => {
    if (!place || saving) return;
    setSaving(true); setNotice("");
    try {
      if (!await setPlaceFavorite(place.id, !favorited)) throw new Error();
      setFavorite({ id: place.id, value: !favorited });
      void client.invalidateQueries({ queryKey: ["places"] });
    } catch (error) {
      setNotice(error instanceof CatalystError && error.status === 401
        ? "Your sign-in expired. Sign in again to save places."
        : "Could not save this place. Please try again.");
    }
    finally { setSaving(false); }
  };
  return <div className="sd__location">
    <div className="sd__scene-card" data-collapsed={userHidden} ref={ref}>
      <div className="sd__scene-heading">
        <button type="button" className="sd__collapse" aria-label={userHidden ? "Show minimap" : "Hide minimap"} aria-expanded={!userHidden} onClick={toggleUserHidden}><span data-collapsed={userHidden}><Icon name="collapse" /></span></button>
        <strong className="sd__place" title={scene.title || "Explore Decentraland"}>{scene.title || "Explore Decentraland"}</strong>
        {favorited && <button type="button" className="sd__favorite" aria-label="Remove place from favorites" disabled={saving || !place || identity.isGuest} aria-busy={saving} onClick={() => void toggleFavorite()}><Icon name="heartFilled" /></button>}
        <button type="button" className="sd__scene-options" aria-label="Scene options" aria-expanded={menu} aria-haspopup="menu" onClick={() => setMenu(value => !value)}><Icon name="sceneOptions" /></button>
      </div>
      <div className="sd__scene-details"><button type="button" disabled={!coords} aria-label="Copy link to this location" onClick={() => void copy(jumpUrl(coords, scene.realm))}><Icon name="location" />{coords || "Locating\u2026"}<span className="sd__copy-feedback" data-copied={copied} role="status">{copied ? "Copied" : <Icon name="copy" />}</span></button><button type="button" className="sd__population" aria-label={String(players.length) + " people nearby, open chat"} onClick={onNearby}><Icon name="people" />{players.length}</button></div>
      {notice && <p className="sd__notice" role="status">{notice}</p>}
      {menu && <div className="sd__scene-menu"><ContextMenu autoFocus onClose={() => setMenu(false)} items={[
        { kind: "button", label: "Copy coordinates", disabled: !coords, onClick: () => void copy(coords) },
        { kind: "button", label: "Copy jump link", disabled: !coords, onClick: () => void copy(jumpUrl(coords, scene.realm)) },
        { kind: "separator" },
        { kind: "button", label: "Send feedback", onClick: () => { setMenu(false); setOwnerModal("feedback"); } },
        { kind: "button", label: "Send tip", disabled: !(owner.tipAddress || owner.address), onClick: () => { setMenu(false); setOwnerModal("tip"); } },
        { kind: "separator" },
        { kind: "button", label: "Time of day", onClick: () => { setMenu(false); onSkybox(); } },
        { kind: "button", label: favorited ? "Remove place from favorites" : "Save place to favorites", disabled: !place || saving || identity.isGuest, onClick: () => { setMenu(false); void toggleFavorite(); } },
      ]} /></div>}
    </div>
    <Minimap place={scene.title ?? ""} coords={coords} heading={position?.heading} inline />
    {ownerModal === "feedback" && <SceneFeedbackModal owner={owner.address} recipients={owner.recipients} loading={owner.loading} sceneTitle={scene.title || owner.sceneTitle || ""} coords={coords} onClose={() => setOwnerModal(null)} />}
    {ownerModal === "tip" && (owner.tipAddress || owner.address) && <SceneTipModal owner={(owner.tipAddress || owner.address)!} recipients={owner.recipients} loading={owner.loading} sceneTitle={scene.title || owner.sceneTitle || ""} coords={coords} onClose={() => setOwnerModal(null)} />}
  </div>;
}
