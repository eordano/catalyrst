import type { MpLaunchRequest } from "./rules";
import type { MpEventFrame } from "./types";

export const MP_PAIRING_KEY = "dcl-mp-testd";

export type MpPairing = { port: number; token: string };

export function readPairing(win: Window): MpPairing | null {
  let fromUrl: MpPairing | null = null;
  try {
    const url = new URL(win.location.href);
    const port = Number(url.searchParams.get("mpd") ?? "");
    const hash = new URLSearchParams(url.hash.replace(/^#/, ""));
    const token = hash.get("mpdtoken") ?? "";
    if (Number.isInteger(port) && port > 0 && port < 65536 && token) {
      fromUrl = { port, token };
    }
  } catch {
  }
  if (fromUrl) {
    try {
      win.localStorage.setItem(MP_PAIRING_KEY, JSON.stringify(fromUrl));
    } catch {
    }
    return fromUrl;
  }
  try {
    const raw = win.localStorage.getItem(MP_PAIRING_KEY);
    if (!raw) return null;
    const p = JSON.parse(raw) as Partial<MpPairing>;
    if (
      Number.isInteger(p.port) &&
      (p.port as number) > 0 &&
      typeof p.token === "string" &&
      p.token
    ) {
      return { port: p.port as number, token: p.token };
    }
  } catch {
  }
  return null;
}

export function clearPairing(win: Window): void {
  try {
    win.localStorage.removeItem(MP_PAIRING_KEY);
  } catch {
  }
}

export class MpApiError extends Error {
  status: number;
  constructor(status: number, message: string) {
    super(message);
    this.status = status;
  }
}

export type MpConnState = "connecting" | "connected" | "down";

export type MpConnectHandlers = {
  onEvent?: (frame: MpEventFrame) => void;
  onState?: (state: MpConnState) => void;
};

const RECONNECT_MIN_MS = 1_000;
const RECONNECT_MAX_MS = 15_000;

export class MpSidecar {
  readonly base: string;
  private token: string;
  private ws: WebSocket | null = null;
  private closed = false;
  private backoff = RECONNECT_MIN_MS;
  private timer: ReturnType<typeof setTimeout> | null = null;

  constructor(pairing: MpPairing) {
    this.base = `http://127.0.0.1:${pairing.port}`;
    this.token = pairing.token;
  }

  private async api<T>(
    path: string,
    init?: { method?: string; body?: unknown },
  ): Promise<T | null> {
    const res = await fetch(`${this.base}${path}`, {
      method: init?.method ?? "GET",
      headers: {
        Authorization: `Bearer ${this.token}`,
        ...(init?.body !== undefined ? { "Content-Type": "application/json" } : {}),
      },
      body: init?.body !== undefined ? JSON.stringify(init.body) : undefined,
    });
    if (res.status === 404) return null;
    if (!res.ok) {
      let detail = res.statusText;
      try {
        const j = (await res.json()) as { error?: string; detail?: string };
        detail = j.detail ?? j.error ?? detail;
      } catch {
      }
      throw new MpApiError(res.status, detail);
    }
    return (await res.json()) as T;
  }

  async presets(): Promise<unknown[]> {
    const r = await this.api<{ presets?: unknown[] }>("/presets");
    return Array.isArray(r?.presets) ? r.presets : [];
  }

  async profiles(): Promise<unknown[]> {
    const r = await this.api<{ profiles?: unknown[] }>("/profiles");
    return Array.isArray(r?.profiles) ? r.profiles : [];
  }

  async runs(): Promise<unknown[]> {
    const r = await this.api<{ runs?: unknown[] }>("/runs");
    return Array.isArray(r?.runs) ? r.runs : [];
  }

  run(id: string) {
    return this.api<unknown>(`/runs/${encodeURIComponent(id)}/run`);
  }

  status(id: string) {
    return this.api<unknown>(`/runs/${encodeURIComponent(id)}/status`);
  }

  report(id: string) {
    return this.api<unknown>(`/runs/${encodeURIComponent(id)}/report`);
  }

  verdict(id: string) {
    return this.api<unknown>(`/runs/${encodeURIComponent(id)}/verdict`);
  }

  async launch(req: MpLaunchRequest): Promise<{ id: string }> {
    const r = await this.api<{ id?: string }>("/runs", {
      method: "POST",
      body: req,
    });
    if (!r?.id) throw new MpApiError(502, "sidecar accepted the run but returned no id");
    return { id: r.id };
  }

  replay(id: string, body: { tier: "a" | "b"; profile?: string; seed?: number }) {
    return this.api<unknown>(`/runs/${encodeURIComponent(id)}/replay`, {
      method: "POST",
      body,
    });
  }

  stop(id: string) {
    return this.api<unknown>(`/runs/${encodeURIComponent(id)}/stop`, {
      method: "POST",
      body: {},
    });
  }

  async artifactBlobUrl(id: string, path: string): Promise<string | null> {
    const res = await fetch(
      `${this.base}/runs/${encodeURIComponent(id)}/artifact/${path}`,
      { headers: { Authorization: `Bearer ${this.token}` } },
    );
    if (!res.ok) return null;
    return URL.createObjectURL(await res.blob());
  }

  async artifactJson<T>(id: string, path: string): Promise<T | null> {
    return this.api<T>(`/runs/${encodeURIComponent(id)}/artifact/${path}`);
  }

  async artifactEntries(
    id: string,
    dir: string,
  ): Promise<Array<{ name: string; dir: boolean }>> {
    const r = await this.artifactJson<{ entries?: unknown[] }>(id, dir);
    if (!Array.isArray(r?.entries)) return [];
    return r.entries
      .map((e) => {
        const o = e as { name?: unknown; dir?: unknown };
        return typeof o?.name === "string"
          ? { name: o.name, dir: o.dir === true }
          : null;
      })
      .filter((e): e is { name: string; dir: boolean } => e !== null);
  }

  async artifactText(id: string, path: string): Promise<string | null> {
    const res = await fetch(
      `${this.base}/runs/${encodeURIComponent(id)}/artifact/${path}`,
      { headers: { Authorization: `Bearer ${this.token}` } },
    );
    if (!res.ok) return null;
    return res.text();
  }

  connect(handlers: MpConnectHandlers): void {
    this.closed = false;
    this.dial(handlers);
  }

  private dial(handlers: MpConnectHandlers): void {
    if (this.closed) return;
    handlers.onState?.("connecting");
    let ws: WebSocket;
    try {
      ws = new WebSocket(
        `ws://${this.base.replace(/^http:\/\//, "")}/events?token=${encodeURIComponent(this.token)}`,
      );
    } catch {
      this.scheduleRedial(handlers);
      return;
    }
    this.ws = ws;
    ws.onopen = () => {
      this.backoff = RECONNECT_MIN_MS;
      try {
        ws.send(JSON.stringify({ type: "auth", token: this.token }));
      } catch {
      }
      handlers.onState?.("connected");
    };
    ws.onmessage = (ev) => {
      if (typeof ev.data !== "string") return;
      try {
        const frame = JSON.parse(ev.data) as MpEventFrame;
        if (frame && typeof frame.type === "string") handlers.onEvent?.(frame);
      } catch {
      }
    };
    ws.onerror = () => {};
    ws.onclose = () => {
      if (this.ws === ws) this.ws = null;
      if (!this.closed) {
        handlers.onState?.("down");
        this.scheduleRedial(handlers);
      }
    };
  }

  private scheduleRedial(handlers: MpConnectHandlers): void {
    if (this.timer) clearTimeout(this.timer);
    this.timer = setTimeout(() => this.dial(handlers), this.backoff);
    this.backoff = Math.min(this.backoff * 2, RECONNECT_MAX_MS);
  }

  close(): void {
    this.closed = true;
    if (this.timer) clearTimeout(this.timer);
    this.timer = null;
    this.ws?.close();
    this.ws = null;
  }
}
