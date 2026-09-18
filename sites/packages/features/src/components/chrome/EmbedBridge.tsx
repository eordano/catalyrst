import { useEffect } from "react";
import { useLocation } from "react-router";

import {
  embedEscapeHref,
  installEmbedEscapeRelay,
  installEmbedLinkGuard,
  isEmbedded,
} from "@ui/web/frames/embed";

export default function EmbedBridge() {
  const location = useLocation();

  useEffect(() => {
    if (typeof window === "undefined" || !isEmbedded()) return;
    const offLinks = installEmbedLinkGuard(window);
    const offEscape = installEmbedEscapeRelay(window);
    return () => {
      offLinks();
      offEscape();
    };
  }, []);

  useEffect(() => {
    if (typeof window === "undefined" || !isEmbedded()) return;
    const href = embedEscapeHref(location.pathname, location.search, location.hash);
    if (href) (window.top ?? window).location.assign(href);
  }, [location]);

  return null;
}
