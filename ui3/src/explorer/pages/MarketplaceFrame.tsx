import { useEffect, useRef, useState } from "react";
import Spinner from "../../atoms/Spinner";
import { siteUrl } from "../../data/site";
import { frameEscapeHref, isEmbedEscapeMessage } from "../../web/frames/embed";
import "./marketplaceframe.css";

const MARKETPLACE_PATH = "/shop";

type MarketplaceFrameProps = {
  src?: string;
};

export default function MarketplaceFrame({ src }: MarketplaceFrameProps) {
  const [loaded, setLoaded] = useState(false);
  const frameRef = useRef<HTMLIFrameElement>(null);
  const url = src ?? siteUrl(MARKETPLACE_PATH);
  const onLoad = () => {
    setLoaded(true);
    const escape = frameEscapeHref(frameRef.current);
    if (escape) (window.top ?? window).location.assign(escape);
  };
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
      <iframe
        ref={frameRef}
        className={"mkf__frame" + (loaded ? " is-loaded" : "")}
        title="Marketplace"
        src={url}
        allow="clipboard-write"
        onLoad={onLoad}
      />
    </div>
  );
}
