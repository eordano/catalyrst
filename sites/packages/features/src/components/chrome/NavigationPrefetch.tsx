import { useEffect, useState } from "react";
import { PrefetchPageLinks, useLocation, useNavigation } from "react-router";
import { chromeHrefTarget } from "./ChromeNavBridge";

const backgroundFetch = { fetchPriority: "low" as const };

export default function NavigationPrefetch() {
  const location = useLocation();
  const navigation = useNavigation();
  const [pages, setPages] = useState<string[]>([]);
  useEffect(() => {
    if (navigation.state !== "idle") return;
    const collect = () => {
      const found = new Set<string>();
      for (const link of document.querySelectorAll<HTMLAnchorElement>('nav a[href], aside a[href], [role="navigation"] a[href], .dtb a[href]')) {
        if (link.hasAttribute("download") || link.getAttribute("aria-disabled") === "true") continue;
        const target = chromeHrefTarget(link.href, window.location.origin);
        if (target.kind !== "app" || target.to === location.pathname + location.search) continue;
        const url = new URL(target.to, window.location.origin);
        if (/\/(logout|sign-out|delete|deploy|publish|scene-editor)(\/|$)/.test(url.pathname) || url.searchParams.has("new")) continue;
        found.add(url.pathname + url.search);
      }
      setPages(previous => {
        const next = [...found];
        return JSON.stringify(previous) === JSON.stringify(next) ? previous : next;
      });
    };
    let timer: ReturnType<typeof setTimeout>;
    const schedule = () => { clearTimeout(timer); timer = setTimeout(collect, 0); };
    const observer = new MutationObserver(schedule);
    const start = () => {
      schedule();
      observer.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["href"] });
    };
    if (document.readyState === "complete") start();
    else window.addEventListener("load", start, { once: true });
    return () => { clearTimeout(timer); observer.disconnect(); window.removeEventListener("load", start); };
  }, [location.pathname, location.search, navigation.state]);
  return <>{navigation.state === "idle" && pages.map(page => <PrefetchPageLinks key={page} page={page} {...backgroundFetch} />)}</>;
}
