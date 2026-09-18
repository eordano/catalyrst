import { useEffect, useLayoutEffect, useRef, useState } from "react";
import Spinner from "../../atoms/Spinner";
import { siteUrl } from "../../data/site";
import { frameEscapeHref, isEmbedEscapeMessage } from "../../web/frames/embed";
import "./marketplaceframe.css";
import { moveMarketplace, warmMarketplace } from "./marketplace-preload";
import { useBridgeState } from "../../overlay/bridge";

const MARKETPLACE_PATH = "/shop";

type MarketplaceFrameProps = {
  src?: string;
};

export default function MarketplaceFrame({ src }: MarketplaceFrameProps) {
  const [loaded, setLoaded] = useState(false);
  const containerRef = useRef<HTMLDivElement>(null);
  const frameRef = useRef<HTMLIFrameElement | null>(null);
  const viewer = useBridgeState(state => state.identity.address);
  const url = src ?? siteUrl(MARKETPLACE_PATH);
  useLayoutEffect(() => {
    const warm = warmMarketplace(viewer ?? null, url);
    frameRef.current = warm.frame;
    const onLoad = () => {
      setLoaded(true);
      const escape = frameEscapeHref(warm.frame);
      if (escape) (window.top ?? window).location.assign(escape);
    };
    warm.frame.className = "mkf__frame";
    warm.frame.addEventListener("load", onLoad);
    if (containerRef.current) moveMarketplace(warm, containerRef.current);
    setLoaded(warm.loaded);
    if (warm.loaded) onLoad();
    return () => {
      warm.frame.removeEventListener("load", onLoad);
      moveMarketplace(warm, warm.parking);
      frameRef.current = null;
    };
  }, [url, viewer]);
  useEffect(() => {
    const onMessage = (e: MessageEvent) => {
      if (!isEmbedEscapeMessage(e, frameRef.current)) return;
      window.dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", code: "Escape", bubbles: true, cancelable: true }));
    };
    window.addEventListener("message", onMessage);
    return () => window.removeEventListener("message", onMessage);
  }, []);
  return (
    <div className="mkf">
      {!loaded && (
        <div className="mkf__loading">
          <Spinner size={34} color="rgba(255,255,255,0.72)" aria-label="Loading marketplace" />
        </div>
      )}
      <div ref={containerRef} className={"mkf__document" + (loaded ? " is-loaded" : "")} />
    </div>
  );
}
