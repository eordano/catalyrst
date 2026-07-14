import { EDITOR_BUS_CHANNEL, type BusEnvelope } from "../generated/editor-bus";
import { SceneToPageMessageSchema } from "../generated/editor-bus-schemas";
import { check } from "../validate";
import { PROJECT_CACHE } from "./project-cache";

export interface BridgePageInfo {
  url: string
  project: string | null
  sceneHash: string | null
}

export interface HelloFrame {
  kind: 'hello'
  token: string
  bridgeVersion: number | null
  page: BridgePageInfo
  sceneReady: boolean
  playing: boolean
  takeover?: boolean
}

export interface HelloOkFrame {
  kind: 'hello-ok'
  serverVersion: string
  heartbeatMs: number
}

export interface BusFrame {
  kind: 'bus'
  msg: unknown
}

export interface EventFrame {
  kind: 'event'
  msg: unknown
}

export type CtlRequest =
  | { op: 'playCtl'; action: 'FreezeScene' | 'UnfreezeScene' | 'TickScene'; ticks?: number }
  | {
      op: 'screenshot'
      maxWidth?: number
      format?: 'png' | 'jpeg'
      quality?: number
      settleMs?: number
    }
  | { op: 'placeAssetPrep'; assetId: string; contents: Record<string, string> }
  | { op: 'pageInfo' }

export interface CtlPlayResult {
  action: 'FreezeScene' | 'UnfreezeScene' | 'TickScene'
  result: string
}

export interface CtlScreenshotResult {
  dataUrl: string
  width: number
  height: number
  sourceWidth: number
  sourceHeight: number
}

export interface CtlPlaceAssetPrepResult {
  baseDir: string | null
  hashes: Record<string, string>
  cached: string[]
  failed: string[]
}

export interface CtlPageInfo extends BridgePageInfo {
  sceneReady: boolean
  playing: boolean
  frozen: boolean | null
  bridgeVersion: number | null
}

export interface CtlFrame {
  kind: 'ctl'
  id: string
  ctl: CtlRequest
}

export interface CtlReplyFrame {
  kind: 'ctl-reply'
  id: string
  ok: boolean
  result?: unknown
  error?: string
}

export interface PingFrame {
  kind: 'ping'
  t: number
}

export interface PongFrame {
  kind: 'pong'
  t: number
}

export type ServerToBridgeFrame = HelloOkFrame | BusFrame | CtlFrame | PingFrame
export type BridgeToServerFrame = HelloFrame | EventFrame | CtlReplyFrame | PongFrame
export type RelayFrame = ServerToBridgeFrame | BridgeToServerFrame

export const RELAY_CLOSE = {
  BAD_TOKEN: 4401,
  BAD_ORIGIN: 4403,
  ALREADY_PAIRED: 4409,
  REPLACED: 4410,
  HEARTBEAT: 4408,
} as const



const BUS_CHANNEL = EDITOR_BUS_CHANNEL;
const LOCALSTORAGE_KEY = "dcl-mcp-relay";
const PLAY_REPLY_TIMEOUT_MS = 5000;
const SCREENSHOT_RETRIES = 5;
const RECONNECT_MIN_MS = 1000;
const RECONNECT_MAX_MS = 15000;

export interface ConnectOptions {
  url?: string | number | null;
  token?: string | null;
  getViewportEl?: () => HTMLIFrameElement | null;
  takeover?: boolean;
}

export interface AutoConnectOptions extends Omit<ConnectOptions, "url" | "token"> {
  /** Consent gate for a NON-loopback relay: resolve true to pair. Absent means
      remote pairing is refused outright -- never silently allowed. */
  confirmRemote?: (host: string) => Promise<boolean>;
}

interface BridgeStatus {
  connected: boolean;
  paired: boolean;
  detachedReason: string | null;
  sceneReady: boolean;
  sceneHash: string | null;
  bridgeVersion: number | null;
}

function resolveWsUrl(v: string | number | null | undefined): string | null {
  if (typeof v === "number" && Number.isInteger(v) && v > 0) {
    return `ws://127.0.0.1:${v}/bridge`;
  }
  if (typeof v === "string" && v.trim() !== "") {
    const s = v.trim();
    if (/^\d+$/.test(s)) return `ws://127.0.0.1:${s}/bridge`;
    if (/^wss?:\/\//.test(s)) return s;
  }
  return null;
}

// Pairing hands the relay screenshots, play control and asset writes, so a URL
// must not be able to choose a REMOTE relay silently: a crafted editor link with
// ?mcp=wss://... and a fragment token would pair the victim's session with an
// attacker's server. Loopback stays silent -- the documented rig workflow, and a
// listener on 127.0.0.1 already means the machine is yours. Anything else needs
// the host's explicit consent, which the CALLER renders (this module owns the
// trust decision, not its presentation).
function isLoopbackWsUrl(wsUrl: string): boolean {
  try {
    const h = new URL(wsUrl).hostname;
    return h === "localhost" || h === "[::1]" || h === "::1" || /^127(\.\d{1,3}){3}$/.test(h);
  } catch {
    return false;
  }
}

export function relayHostOf(wsUrl: string): string {
  try {
    return new URL(wsUrl).host;
  } catch {
    return wsUrl;
  }
}

function readStoredConfig(): { url: string | null; token: string | null } {
  try {
    const raw = window.localStorage?.getItem(LOCALSTORAGE_KEY);
    if (!raw) return { url: null, token: null };
    const parsed = JSON.parse(raw) as { url?: unknown; token?: unknown };
    return {
      url: typeof parsed.url === "string" || typeof parsed.url === "number" ? String(parsed.url) : null,
      token: typeof parsed.token === "string" ? parsed.token : null,
    };
  } catch {
    return { url: null, token: null };
  }
}

function pageProject(): string | null {
  try {
    return new URLSearchParams(window.location.search).get("project");
  } catch {
    return null;
  }
}

let activeDispose: (() => void) | null = null;

export function autoConnect(opts: AutoConnectOptions = {}): (() => void) | null {
  if (typeof window === "undefined") return null;
  const { confirmRemote, ...connectOpts } = opts;
  const mcpParam = new URLSearchParams(window.location.search).get("mcp");
  const hashToken = new URLSearchParams(window.location.hash.replace(/^#/, "")).get("mcptoken");
  const stored = readStoredConfig();
  const url = resolveWsUrl(mcpParam ?? stored.url);
  const token = hashToken ?? stored.token;
  if (!url || !token) return null;
  if (isLoopbackWsUrl(url)) return connect({ ...connectOpts, url, token });
  if (confirmRemote === undefined) {
    console.warn(`[mcp-bridge] remote relay ${relayHostOf(url)} refused: no consent surface`);
    return null;
  }
  let cancelled = false;
  let disposeConnection: (() => void) | null = null;
  void confirmRemote(relayHostOf(url)).then((approved) => {
    if (approved && !cancelled) disposeConnection = connect({ ...connectOpts, url, token });
  });
  return () => {
    cancelled = true;
    disposeConnection?.();
    disposeConnection = null;
  };
}

export function connect(opts: ConnectOptions): () => void {
  const url = resolveWsUrl(opts.url) ?? resolveWsUrl(readStoredConfig().url);
  const token = (typeof opts.token === "string" && opts.token) || readStoredConfig().token;
  if (!url || !token) {
    console.warn("[mcp-bridge] missing relay url or token \u{2014} not connecting");
    return () => {};
  }
  activeDispose?.();

  let ws: WebSocket | null = null;
  let disposed = false;
  let detachedReason: string | null = null;
  let reconnectMs = RECONNECT_MIN_MS;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let paired = false;

  let sceneReady = false;
  let sceneHash: string | null = null;
  let bridgeVersion: number | null = null;
  let frozen: boolean | null = null;
  const playing = false;

  const rpcTag = `mcpb${Math.random().toString(36).slice(2, 8)}`;
  let rpcSeq = 0;
  const rpcPending = new Map<
    string,
    { resolve: (v: unknown) => void; reject: (e: Error) => void; timer: ReturnType<typeof setTimeout> }
  >();

  const channel = new BroadcastChannel(BUS_CHANNEL);

  let badge: HTMLDivElement | null = null;
  const setBadge = (text: string, color: string) => {
    try {
      if (!badge) {
        badge = document.createElement("div");
        badge.setAttribute("data-mcp-bridge-badge", "");
        badge.style.cssText =
          "position:fixed;right:10px;bottom:10px;z-index:2147483000;padding:2px 8px;" +
          "font:11px/1.6 monospace;border-radius:4px;background:rgba(0,0,0,.65);" +
          "pointer-events:none;user-select:none;";
        document.body.appendChild(badge);
      }
      badge.textContent = text;
      badge.style.color = color;
    } catch {
    }
  };
  const removeBadge = () => {
    badge?.remove();
    badge = null;
  };

  const wsSend = (frame: BridgeToServerFrame) => {
    if (ws && ws.readyState === WebSocket.OPEN) {
      try {
        ws.send(JSON.stringify(frame));
      } catch {
      }
    }
  };

  channel.onmessage = (ev: MessageEvent) => {
    const env = (ev?.data ?? null) as BusEnvelope | null;
    // Coarse guard first and unchanged: the relay posts `{to:"scene", ...}` on
    // this same channel with a payload it does not own, and that traffic is
    // meant to be ignored here rather than rejected against a page-bound shape.
    if (!env || typeof env !== "object" || env.to !== "page" || !env.msg) return;
    // A distinct boundary id from editor-bus.ts: both receive the same shape,
    // but only this one forwards it to the MCP relay below, so a production
    // report has to say which receiver saw the drift.
    const msg = check(SceneToPageMessageSchema, env.msg, "mcp-bridge/scene-to-page");
    if (msg.type === "scene-ready") {
      sceneReady = true;
      bridgeVersion = typeof msg.bridge === "number" ? msg.bridge : null;
      sceneHash = msg.scene && typeof msg.scene.hash === "string" ? msg.scene.hash : null;
      frozen = typeof msg.frozen === "boolean" ? msg.frozen : null;
    }
    if (msg.type === "rpc-reply" && typeof msg.id === "string" && rpcPending.has(msg.id)) {
      const entry = rpcPending.get(msg.id);
      rpcPending.delete(msg.id);
      if (entry) {
        clearTimeout(entry.timer);
        if (msg.ok) entry.resolve(msg.result);
        else entry.reject(new Error(String(msg.error ?? "rpc failed")));
      }
      return;
    }
    wsSend({ kind: "event", msg });
  };

  const busPost = (msg: unknown) => {
    try {
      channel.postMessage({ to: "scene", msg });
    } catch {
    }
  };

  const busRpc = (method: string, args: unknown[], timeoutMs: number): Promise<unknown> => {
    const id = `${rpcTag}-${++rpcSeq}`;
    return new Promise<unknown>((resolve, reject) => {
      const timer = setTimeout(() => {
        if (rpcPending.has(id)) {
          rpcPending.delete(id);
          reject(new Error(`rpc timeout: ${method}`));
        }
      }, timeoutMs);
      rpcPending.set(id, { resolve, reject, timer });
      busPost({ type: "rpc", id, method, args });
    });
  };

  const getViewport = (): HTMLIFrameElement => {
    const el =
      (opts.getViewportEl ? opts.getViewportEl() : null) ??
      document.querySelector<HTMLIFrameElement>('iframe[src*="/_play"]');
    if (!el || !el.contentWindow) {
      throw new Error("viewport iframe (/_play) not found or not loaded");
    }
    return el;
  };

  const playCtl = (action: "FreezeScene" | "UnfreezeScene" | "TickScene", ticks?: number): Promise<CtlPlayResult> => {
    const frame = getViewport();
    const cw = frame.contentWindow as Window;
    const requestId = `${rpcTag}-play-${++rpcSeq}`;
    return new Promise<CtlPlayResult>((resolve, reject) => {
      const onReply = (ev: MessageEvent) => {
        const d = ev?.data as
          | { type?: unknown; requestId?: unknown; ok?: unknown; result?: unknown; error?: unknown }
          | null;
        if (!d || d.type !== "dcl-bridge-reply" || d.requestId !== requestId) return;
        window.removeEventListener("message", onReply);
        clearTimeout(timer);
        if (d.ok) {
          if (action === "FreezeScene") frozen = true;
          if (action === "UnfreezeScene") frozen = false;
          resolve({ action, result: String(d.result ?? "") });
        } else {
          reject(new Error(String(d.error ?? `${action} failed`)));
        }
      };
      const timer = setTimeout(() => {
        window.removeEventListener("message", onReply);
        reject(new Error(`${action}: no dcl-bridge-reply within ${PLAY_REPLY_TIMEOUT_MS}ms`));
      }, PLAY_REPLY_TIMEOUT_MS);
      window.addEventListener("message", onReply);
      const msg: Record<string, unknown> = { type: "dcl-bridge", action, requestId };
      if (action === "TickScene") msg["count"] = Math.max(1, Math.trunc(ticks ?? 1));
      let target = "*";
      try {
        target = new URL(frame.src, window.location.href).origin;
      } catch {
      }
      cw.postMessage(msg, target);
    });
  };

  const sleep = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

  const captureOnce = (
    cw: Window,
    canvas: HTMLCanvasElement,
    maxWidth: number,
    format: "png" | "jpeg",
    quality: number,
  ): Promise<{ blank: boolean; result: CtlScreenshotResult }> =>
    new Promise((resolve, reject) => {
      cw.requestAnimationFrame(() => {
        try {
          const sw = canvas.width;
          const sh = canvas.height;
          if (!sw || !sh) throw new Error("viewport canvas has zero size");
          const scale = maxWidth > 0 && sw > maxWidth ? maxWidth / sw : 1;
          const out = document.createElement("canvas");
          out.width = Math.max(1, Math.round(sw * scale));
          out.height = Math.max(1, Math.round(sh * scale));
          const g = out.getContext("2d");
          if (!g) throw new Error("2d context unavailable");
          g.drawImage(canvas, 0, 0, out.width, out.height);
          const sample = g.getImageData(0, 0, out.width, out.height).data;
          let opaque = 0;
          const stride = Math.max(4, Math.floor(sample.length / 4 / 400)) * 4;
          for (let i = 3; i < sample.length; i += stride) {
            if ((sample[i] as number) > 8) opaque += 1;
          }
          const mime = format === "jpeg" ? "image/jpeg" : "image/png";
          resolve({
            blank: opaque === 0,
            result: {
              dataUrl: out.toDataURL(mime, quality),
              width: out.width,
              height: out.height,
              sourceWidth: sw,
              sourceHeight: sh,
            },
          });
        } catch (e) {
          reject(e instanceof Error ? e : new Error(String(e)));
        }
      });
    });

  const screenshot = async (req: Extract<CtlRequest, { op: "screenshot" }>): Promise<CtlScreenshotResult> => {
    const frame = getViewport();
    const cw = frame.contentWindow as Window;
    const doc = frame.contentDocument;
    const canvas = doc?.getElementById("mygame-canvas") as HTMLCanvasElement | null;
    if (!canvas) throw new Error("#mygame-canvas not found in the /_play iframe");
    if (req.settleMs && req.settleMs > 0) await sleep(req.settleMs);
    const maxWidth = req.maxWidth ?? 1024;
    const format = req.format ?? "png";
    const quality = req.quality ?? 0.9;
    let last: CtlScreenshotResult | null = null;
    for (let attempt = 0; attempt < SCREENSHOT_RETRIES; attempt += 1) {
      const { blank, result } = await captureOnce(cw, canvas, maxWidth, format, quality);
      if (!blank) return result;
      last = result;
      await sleep(120);
    }
    void last;
    throw new Error(
      `E_BLANK_FRAME: ${SCREENSHOT_RETRIES} capture attempts returned fully-transparent frames \u{2014} is the engine rendering? (WebGPU frames are only readable inside the iframe's rAF)`,
    );
  };

  const placeAssetPrep = async (
    req: Extract<CtlRequest, { op: "placeAssetPrep" }>,
  ): Promise<CtlPlaceAssetPrepResult> => {
    const persisted = (await busRpc("initAsset", [req.assetId, req.contents], 15000)) as {
      baseDir?: unknown;
      hashes?: unknown;
    } | null;
    const baseDir =
      persisted && typeof persisted.baseDir === "string" && persisted.baseDir
        ? persisted.baseDir.replace(/\/+$/, "")
        : null;
    const hashes: Record<string, string> =
      persisted && persisted.hashes && typeof persisted.hashes === "object"
        ? (persisted.hashes as Record<string, string>)
        : {};
    const cached: string[] = [];
    const failed: string[] = [];
    if (baseDir === null) {
      return { baseDir, hashes, cached, failed: Object.values(req.contents) };
    }
    let contentBase: string | null = null;
    try {
      const about = await fetch("/_project/about", { cache: "no-store" });
      if (about.ok) {
        const j = (await about.json()) as { content?: { publicUrl?: unknown } };
        const pub = j?.content?.publicUrl;
        if (typeof pub === "string" && pub) contentBase = pub.replace(/\/$/, "");
      }
    } catch {
      contentBase = null;
    }
    const cache = contentBase && typeof caches !== "undefined" ? await caches.open(PROJECT_CACHE).catch(() => null) : null;
    if (!contentBase || !cache) {
      return { baseDir, hashes, cached, failed: Object.values(req.contents) };
    }
    await Promise.all(
      Object.entries(req.contents).map(async ([path, cid]) => {
        if (typeof cid !== "string" || !cid) return;
        try {
          const r = await fetch(`/builder-items/${cid}`, { credentials: "omit" });
          if (!r.ok) throw new Error(String(r.status));
          const buf = await r.arrayBuffer();
          const put = (h: string) =>
            cache.put(
              `${contentBase}/contents/${h}`,
              new Response(buf.slice(0), {
                headers: {
                  "content-type": "application/octet-stream",
                  "access-control-allow-origin": "*",
                },
              }),
            );
          await put(cid);
          const mh = hashes[`${baseDir}/${path}`.toLowerCase()];
          if (mh && mh !== cid) await put(mh);
          cached.push(cid);
        } catch {
          failed.push(cid);
        }
      }),
    );
    return { baseDir, hashes, cached, failed };
  };

  const pageInfo = (): CtlPageInfo => ({
    url: window.location.href,
    project: pageProject(),
    sceneHash,
    sceneReady,
    playing,
    frozen,
    bridgeVersion,
  });

  const runCtl = async (req: CtlRequest): Promise<unknown> => {
    switch (req.op) {
      case "playCtl":
        return playCtl(req.action, req.ticks);
      case "screenshot":
        return screenshot(req);
      case "placeAssetPrep":
        return placeAssetPrep(req);
      case "pageInfo":
        return pageInfo();
      default:
        throw new Error(`unknown ctl op: ${String((req as { op?: unknown }).op)}`);
    }
  };

  const openSocket = () => {
    if (disposed || detachedReason) return;
    setBadge("MCP \u{2026}", "#e8c268");
    let socket: WebSocket;
    try {
      socket = new WebSocket(url);
    } catch {
      scheduleReconnect();
      return;
    }
    ws = socket;
    socket.onopen = () => {
      const hello: HelloFrame = {
        kind: "hello",
        token,
        bridgeVersion,
        page: { url: window.location.href, project: pageProject(), sceneHash },
        sceneReady,
        playing,
        ...(opts.takeover ? { takeover: true } : {}),
      };
      socket.send(JSON.stringify(hello));
    };
    socket.onmessage = (ev: MessageEvent) => {
      let frame: ServerToBridgeFrame | null = null;
      try {
        const parsed = JSON.parse(String(ev.data)) as { kind?: unknown };
        if (parsed && typeof parsed.kind === "string") frame = parsed as ServerToBridgeFrame;
      } catch {
        frame = null;
      }
      if (!frame) return;
      switch (frame.kind) {
        case "hello-ok":
          paired = true;
          reconnectMs = RECONNECT_MIN_MS;
          setBadge("MCP \u{25CF}", "#4ade80");
          break;
        case "bus":
          busPost(frame.msg);
          break;
        case "ctl": {
          const id = frame.id;
          runCtl(frame.ctl)
            .then((result) => wsSend({ kind: "ctl-reply", id, ok: true, result }))
            .catch((e: unknown) =>
              wsSend({ kind: "ctl-reply", id, ok: false, error: e instanceof Error ? e.message : String(e) }),
            );
          break;
        }
        case "ping":
          wsSend({ kind: "pong", t: frame.t });
          break;
        default:
          break;
      }
    };
    socket.onclose = (ev: CloseEvent) => {
      if (ws === socket) ws = null;
      paired = false;
      if (
        ev.code === RELAY_CLOSE.BAD_TOKEN ||
        ev.code === RELAY_CLOSE.BAD_ORIGIN ||
        ev.code === RELAY_CLOSE.ALREADY_PAIRED ||
        ev.code === RELAY_CLOSE.REPLACED
      ) {
        detachedReason = ev.reason || `close ${ev.code}`;
        setBadge("MCP detached", "#f87171");
        console.warn(`[mcp-bridge] detached: ${detachedReason}`);
        return;
      }
      scheduleReconnect();
    };
    socket.onerror = () => {
    };
  };

  const scheduleReconnect = () => {
    if (disposed || detachedReason || reconnectTimer !== null) return;
    setBadge("MCP \u{25CB}", "#e8c268");
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      openSocket();
    }, reconnectMs);
    reconnectMs = Math.min(RECONNECT_MAX_MS, reconnectMs * 2);
  };

  openSocket();

  const dispose = () => {
    if (disposed) return;
    disposed = true;
    if (reconnectTimer !== null) clearTimeout(reconnectTimer);
    for (const [, entry] of rpcPending) {
      clearTimeout(entry.timer);
      entry.reject(new Error("mcp bridge disposed"));
    }
    rpcPending.clear();
    try {
      ws?.close(1000, "bridge disposed");
    } catch {
    }
    ws = null;
    channel.onmessage = null;
    try {
      channel.close();
    } catch {
    }
    removeBadge();
    if (activeDispose === dispose) activeDispose = null;
  };
  activeDispose = dispose;

  bridgeApi.status = (): BridgeStatus => ({
    connected: ws !== null && ws.readyState === WebSocket.OPEN,
    paired,
    detachedReason,
    sceneReady,
    sceneHash,
    bridgeVersion,
  });

  return dispose;
}

interface BridgeApi {
  connect(urlOrOpts: string | number | ConnectOptions, token?: string): () => void;
  autoConnect: typeof autoConnect;
  disconnect(): void;
  status: (() => BridgeStatus) | null;
}

const bridgeApi: BridgeApi = {
  connect(urlOrOpts, token) {
    if (typeof urlOrOpts === "object" && urlOrOpts !== null) return connect(urlOrOpts);
    return connect({ url: urlOrOpts, token: token ?? null });
  },
  autoConnect,
  disconnect() {
    activeDispose?.();
  },
  status: null,
};

declare global {
  interface Window {
    __dclMcpBridge?: BridgeApi;
  }
}

if (typeof window !== "undefined") {
  window.__dclMcpBridge = bridgeApi;
}
