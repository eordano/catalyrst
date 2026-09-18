const EMBED_ATTR = "data-embed";
const EMBED_SCOPE_PREFIXES = ["/shop", "/marketplace"] as const;

export const EMBED_BOOT_SCRIPT =
  '(function(){try{if(self!==top)document.documentElement.setAttribute("data-embed","1")}catch(e){}})()';

export function isEmbedScopePath(pathname: string): boolean {
  return EMBED_SCOPE_PREFIXES.some((p) => pathname === p || pathname.startsWith(`${p}/`));
}

export function isEmbedded(doc: Document | undefined = typeof document === "undefined" ? undefined : document): boolean {
  return doc?.documentElement.getAttribute(EMBED_ATTR) === "1";
}

type EmbedLinkDisposition = "frame" | "top" | "blank";

export function embedLinkDisposition(href: string, origin: string): EmbedLinkDisposition {
  let url: URL;
  try {
    url = new URL(href, origin);
  } catch {
    return "frame";
  }
  if (url.protocol !== "http:" && url.protocol !== "https:") return "frame";
  if (url.origin !== origin) return "blank";
  return isEmbedScopePath(url.pathname) ? "frame" : "top";
}

export function embedEscapeHref(pathname: string, search = "", hash = ""): string | null {
  return isEmbedScopePath(pathname) ? null : `${pathname}${search}${hash}`;
}

export function installEmbedLinkGuard(win: Window): () => void {
  const onClick = (e: MouseEvent) => {
    if (e.defaultPrevented || e.button !== 0 || e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
    const el = e.target instanceof Element ? e.target.closest("a[href]") : null;
    if (!(el instanceof HTMLAnchorElement) || el.hasAttribute("download")) return;
    const target = el.getAttribute("target");
    if (target && target !== "_self") return;
    const disposition = embedLinkDisposition(el.href, win.location.origin);
    if (disposition === "frame") return;
    e.preventDefault();
    e.stopPropagation();
    if (disposition === "top") (win.top ?? win).location.assign(el.href);
    else win.open(el.href, "_blank", "noopener");
  };
  win.addEventListener("click", onClick, true);
  return () => win.removeEventListener("click", onClick, true);
}

export const EMBED_ESCAPE_MESSAGE = "dcl-embed-escape";

export function installEmbedEscapeRelay(win: Window): () => void {
  const onKey = (e: KeyboardEvent) => {
    if (e.key !== "Escape" || e.defaultPrevented) return;
    const parent = win.parent;
    if (!parent || parent === win) return;
    parent.postMessage(EMBED_ESCAPE_MESSAGE, "*");
  };
  win.addEventListener("keydown", onKey);
  return () => win.removeEventListener("keydown", onKey);
}

export function isEmbedEscapeMessage(e: MessageEvent, frame: HTMLIFrameElement | null): boolean {
  const from = frame?.contentWindow;
  return e.data === EMBED_ESCAPE_MESSAGE && from != null && e.source === from;
}

export function frameEscapeHref(frame: HTMLIFrameElement | null): string | null {
  try {
    const loc = frame?.contentWindow?.location;
    if (!loc || loc.href === "about:blank") return null;
    if (loc.protocol !== "http:" && loc.protocol !== "https:") return null;
    return isEmbedScopePath(loc.pathname) ? null : loc.href;
  } catch {
    return null;
  }
}
