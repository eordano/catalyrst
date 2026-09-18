import { siteUrl } from "../../data/site";

type WarmFrame = { frame: HTMLIFrameElement; parking: HTMLDivElement; loaded: boolean; viewer: string | null; url: string };
let current: WarmFrame | undefined;

export function moveMarketplace(warm: WarmFrame, parent: HTMLElement) {
  if (typeof parent.moveBefore === "function" && warm.frame.isConnected && parent.isConnected) {
    parent.moveBefore(warm.frame, null);
  } else {
    warm.loaded = false;
    parent.append(warm.frame);
  }
}

export function warmMarketplace(viewer: string | null = null, url = siteUrl("/shop")): WarmFrame {
  if (current?.url === url && current.viewer === viewer) return current;
  current?.frame.remove();
  current?.parking.remove();
  const parking = document.createElement("div");
  parking.hidden = true;
  parking.inert = true;
  parking.dataset.prefetchPanel = "marketplace";
  const frame = document.createElement("iframe");
  frame.title = "Marketplace";
  frame.setAttribute("credentialless", "");
  frame.allow = "clipboard-write";
  frame.src = url;
  const result = { frame, parking, loaded: false, viewer, url };
  frame.addEventListener("load", () => { result.loaded = true; });
  parking.append(frame);
  document.body.append(parking);
  current = result;
  return result;
}
