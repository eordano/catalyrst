import { useEffect, useLayoutEffect, useRef, useState } from "react";
import { useQuery } from "@tanstack/react-query";
import { attachBridge, sendBridge } from "../../../overlay/bridge";
import { destinationLabel, fetchDestinationScene, type SceneDestination } from "../../../data/catalyst/destinationScene";
import { LobbyIcon, PlaceImage } from "./LobbyCards";

export default function LobbySceneCard({ destination, current, onEnter }: {
  destination: SceneDestination;
  current?: { title: string; coords: string };
  onEnter: () => void;
}) {
  const details = useQuery({
    queryKey: ["lobby-destination", destination.realm ?? "", destination.coords ?? ""],
    queryFn: ({ signal }) => fetchDestinationScene(destination, { signal }),
    enabled: !current,
    staleTime: 60000,
    retry: false,
  });
  const [screenshot, setScreenshot] = useState<string>();
  const resuming = !!current;
  const viewport = useRef<HTMLSpanElement>(null);
  const [live, setLive] = useState(false);
  useLayoutEffect(() => {
    if (!resuming || !viewport.current) return;
    const canvas = document.getElementById("mygame-canvas");
    if (!(canvas instanceof HTMLCanvasElement) || !canvas.parentNode) return;
    const parent = canvas.parentNode;
    const next = canvas.nextSibling;
    const tabIndex = canvas.getAttribute("tabindex");
    canvas.tabIndex = -1;
    viewport.current.appendChild(canvas);
    setLive(true);
    return () => {
      parent.insertBefore(canvas, next?.parentNode === parent ? next : null);
      if (tabIndex === null) canvas.removeAttribute("tabindex");
      else canvas.setAttribute("tabindex", tabIndex);
      setLive(false);
    };
  }, [resuming]);
  useEffect(() => {
    if (!resuming || document.getElementById("mygame-canvas") instanceof HTMLCanvasElement) return;
    let requested = false;
    setScreenshot(undefined);
    return attachBridge(push => {
      const photo = push as { kind?: string; dataUrl?: string };
      if (requested && photo.kind === "photo" && photo.dataUrl?.startsWith("data:image/")) {
        requested = false;
        setScreenshot(photo.dataUrl);
      }
    }, () => { requested = true; sendBridge("CapturePhoto"); });
  }, [resuming, current?.title]);
  const title = current?.title || details.data?.title || destinationLabel(destination);
  const coords = current?.coords || destination.coords;
  return <button type="button" className="lh__place lh__place--hero" onClick={onEnter} aria-label={resuming ? `Resume ${title}` : `Jump in to ${title}`}>
    {resuming && <span ref={viewport} className="lh__scene-viewport" role="img" aria-label={`Live view of ${title}`} hidden={!live} />}
    {!live && (screenshot ? <img src={screenshot} alt={`Current view of ${title}`} /> : <PlaceImage src={details.data?.image} />)}
    {live && <span className="lh__scene-live">Live scene</span>}
    <span className="lh__place-copy"><strong>{title}</strong><small>{coords || details.data?.creator || (destination.realm ? destinationLabel(destination) : "Decentraland")}</small></span>
    <span className="lh__jump" aria-hidden="true"><span>{resuming ? "Resume" : "Jump in"}</span><LobbyIcon name="jump" /></span>
  </button>;
}
