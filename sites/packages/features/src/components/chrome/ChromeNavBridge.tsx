import { useCallback, useMemo } from "react";
import type { ComponentPropsWithRef, ReactNode } from "react";
import { Link, useNavigate } from "react-router";

import { ChromeNavContext, isSameDocumentHref, type ChromeNavigate } from "@ui/web/frames/chrome-nav";

const SERVED_OUTSIDE_THE_APP = ["/docs", "/play"] as const;

type ChromeHrefTarget = { kind: "app"; to: string } | { kind: "document" };

export function chromeHrefTarget(href: string, origin: string): ChromeHrefTarget {
  let url: URL;
  try {
    url = new URL(href, origin);
  } catch {
    return { kind: "document" };
  }
  if (url.origin !== origin) return { kind: "document" };
  const outside = SERVED_OUTSIDE_THE_APP.some(
    (prefix) => url.pathname === prefix || url.pathname.startsWith(`${prefix}/`),
  );
  if (outside) return { kind: "document" };
  return { kind: "app", to: `${url.pathname}${url.search}${url.hash}` };
}

function SiteLink({ href = "", download, ...props }: ComponentPropsWithRef<"a">) {
  if (download != null || !isSameDocumentHref(href) ||
    chromeHrefTarget(href, "https://sites.invalid").kind === "document") {
    return <a href={href} download={download} {...props} />;
  }
  return <Link to={href} prefetch="intent" {...props} />;
}

export default function ChromeNavBridge({ children }: { children: ReactNode }) {
  const navigate = useNavigate();
  const go = useCallback<ChromeNavigate>(
    (href) => {
      if (typeof window === "undefined") return false;
      const target = chromeHrefTarget(href, window.location.origin);
      if (target.kind !== "app") return false;
      void navigate(target.to);
      return true;
    },
    [navigate],
  );
  const value = useMemo(() => ({ navigate: go, Link: SiteLink }), [go]);
  return <ChromeNavContext.Provider value={value}>{children}</ChromeNavContext.Provider>;
}
