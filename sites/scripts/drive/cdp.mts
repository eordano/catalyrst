import { spawn, spawnSync, type ChildProcess } from "node:child_process";
import crypto from "node:crypto";
import http from "node:http";
import net from "node:net";

export function freePort(): Promise<number> {
  return new Promise((resolve, reject) => {
    const srv = net.createServer();
    srv.listen(0, "127.0.0.1", () => {
      const address = srv.address();
      const port = typeof address === "object" && address ? address.port : 0;
      srv.close(() => resolve(port));
    });
    srv.on("error", reject);
  });
}

function httpJson(url: string, { method = "GET" }: { method?: string } = {}): Promise<any> {
  return new Promise((resolve, reject) => {
    const req = http.request(url, { method }, (res) => {
      let body = "";
      res.on("data", (c) => (body += c));
      res.on("end", () => {
        try {
          resolve(JSON.parse(body));
        } catch {
          reject(new Error(`bad JSON from ${url}: ${body.slice(0, 120)}`));
        }
      });
    });
    req.on("error", reject);
    req.end();
  });
}

function chromiumCommand(): string[] {
  if (process.env.DCL_SHOT_CHROMIUM) return [process.env.DCL_SHOT_CHROMIUM];
  const which = spawnSync("which", ["chromium"], { encoding: "utf8" });
  if (which.status === 0) return ["chromium"];
  return ["nix", "run", "nixpkgs#chromium", "--"];
}

export type LaunchChromiumOpts = {
  port: number;
  profileDir: string;
  extraArgs?: string[];
};

export async function launchChromium({ port, profileDir, extraArgs = [] }: LaunchChromiumOpts): Promise<ChildProcess> {
  const [bin, ...pre] = chromiumCommand();
  if (!bin) throw new Error("no chromium binary resolved");
  const proc = spawn(
    bin,
    [
      ...pre,
      "--headless=new",
      `--remote-debugging-port=${port}`,
      `--user-data-dir=${profileDir}`,
      "--no-first-run",
      "--disable-gpu",
      "--window-size=1600,1000",
      ...extraArgs,
      "about:blank",
    ],
    { stdio: "ignore" },
  );
  const launchTimeoutMs = Number(process.env.CDP_LAUNCH_TIMEOUT_MS || 120_000);
  const deadline = Date.now() + launchTimeoutMs;
  for (;;) {
    try {
      await httpJson(`http://127.0.0.1:${port}/json/version`);
      break;
    } catch {
      if (Date.now() > deadline) {
        proc.kill();
        throw new Error(`chromium did not expose CDP in ${Math.round(launchTimeoutMs / 1000)}s`);
      }
      await new Promise((r) => setTimeout(r, 400));
    }
  }
  return proc;
}

// A raw CDP event/response frame. The CDP wire protocol is method-keyed and
// heterogeneous by design (params/result shape depends on `method`), so this
// stays loosely typed rather than modeling the whole protocol.
export type CdpMessage = {
  id?: number;
  method?: string;
  params?: Record<string, any>;
  result?: Record<string, any>;
  error?: unknown;
};

class WS {
  sock: net.Socket;
  buf: Buffer;
  queue: string[];
  waiter: ((msg: string) => void) | null;

  constructor(socket: net.Socket, leftover: Buffer) {
    this.sock = socket;
    this.buf = leftover;
    this.queue = [];
    this.waiter = null;
    socket.on("data", (chunk: Buffer) => {
      this.buf = Buffer.concat([this.buf, chunk]);
      this.drain();
    });
    socket.on("error", () => {});
  }

  static connect(wsUrl: string): Promise<WS> {
    return new Promise((resolve, reject) => {
      const u = new URL(wsUrl);
      const sock = net.createConnection(
        { host: u.hostname, port: Number(u.port || 80) },
        () => {
          const key = crypto.randomBytes(16).toString("base64");
          sock.write(
            `GET ${u.pathname}${u.search} HTTP/1.1\r\n` +
              `Host: ${u.host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n` +
              `Sec-WebSocket-Key: ${key}\r\nSec-WebSocket-Version: 13\r\n\r\n`,
          );
        },
      );
      sock.once("error", reject);
      let hs = Buffer.alloc(0);
      const onData = (chunk: Buffer) => {
        hs = Buffer.concat([hs, chunk]);
        const idx = hs.indexOf("\r\n\r\n");
        if (idx !== -1) {
          sock.off("data", onData);
          resolve(new WS(sock, hs.subarray(idx + 4)));
        }
      };
      sock.on("data", onData);
    });
  }

  drain(): void {
    for (;;) {
      const msg = this.tryReadFrame();
      if (msg === null) return;
      if (msg !== undefined) this.queue.push(msg);
      if (this.waiter && this.queue.length) {
        const w = this.waiter;
        this.waiter = null;
        const next = this.queue.shift();
        if (next !== undefined) w(next);
      }
    }
  }

  tryReadFrame(): string | null | undefined {
    if (this.buf.length < 2) return null;
    const opcode = this.buf[0]! & 0x0f;
    let len = this.buf[1]! & 0x7f;
    let off = 2;
    if (len === 126) {
      if (this.buf.length < 4) return null;
      len = this.buf.readUInt16BE(2);
      off = 4;
    } else if (len === 127) {
      if (this.buf.length < 10) return null;
      len = Number(this.buf.readBigUInt64BE(2));
      off = 10;
    }
    if (this.buf.length < off + len) return null;
    const payload = this.buf.subarray(off, off + len);
    this.buf = this.buf.subarray(off + len);
    if (opcode === 0x1) return payload.toString("utf8");
    return undefined;
  }

  send(text: string): void {
    const data = Buffer.from(text);
    const mask = crypto.randomBytes(4);
    let header: Buffer;
    if (data.length < 126) {
      header = Buffer.from([0x81, 0x80 | data.length]);
    } else if (data.length < 65536) {
      header = Buffer.alloc(4);
      header[0] = 0x81;
      header[1] = 0x80 | 126;
      header.writeUInt16BE(data.length, 2);
    } else {
      header = Buffer.alloc(10);
      header[0] = 0x81;
      header[1] = 0x80 | 127;
      header.writeBigUInt64BE(BigInt(data.length), 2);
    }
    const masked = Buffer.from(data);
    for (let i = 0; i < masked.length; i++) masked[i] = masked[i]! ^ mask[i % 4]!;
    this.sock.write(Buffer.concat([header, mask, masked]));
  }

  next(timeoutMs: number): Promise<string> {
    if (this.queue.length) return Promise.resolve(this.queue.shift()!);
    return new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        this.waiter = null;
        reject(new Error("ws recv timeout"));
      }, timeoutMs);
      this.waiter = (msg: string) => {
        clearTimeout(timer);
        resolve(msg);
      };
    });
  }
}

export class Tab {
  ws: WS;
  id: number;
  events: CdpMessage[];

  constructor(ws: WS) {
    this.ws = ws;
    this.id = 0;
    this.events = [];
  }

  noteEvent(obj: CdpMessage): void {
    if (!obj.method) return;
    this.events.push(obj);
    if (this.events.length > 1000) this.events.splice(0, 200);
  }

  drainEvents(): CdpMessage[] {
    while (this.ws.queue.length) {
      const raw = this.ws.queue.shift();
      if (raw === undefined) break;
      try {
        this.noteEvent(JSON.parse(raw));
      } catch {
      }
    }
    const out = this.events;
    this.events = [];
    return out;
  }

  static async open(port: number, url = "about:blank"): Promise<Tab> {
    const info = await httpJson(
      `http://127.0.0.1:${port}/json/new?${encodeURIComponent(url)}`,
      { method: "PUT" },
    );
    const tab = new Tab(await WS.connect(info.webSocketDebuggerUrl));
    await tab.cmd("Page.enable");
    await tab.cmd("Runtime.enable");
    return tab;
  }

  async cmd(method: string, params: Record<string, unknown> = {}, timeoutMs = 60_000): Promise<Record<string, any>> {
    const id = ++this.id;
    this.ws.send(JSON.stringify({ id, method, params }));
    const deadline = Date.now() + timeoutMs;
    for (;;) {
      const raw = await this.ws.next(Math.max(200, deadline - Date.now()));
      let obj: CdpMessage;
      try {
        obj = JSON.parse(raw);
      } catch {
        continue;
      }
      if (obj.id === id) {
        if (obj.error) throw new Error(`${method}: ${JSON.stringify(obj.error)}`);
        return obj.result ?? {};
      }
      this.noteEvent(obj);
    }
  }

  async ev(expression: string, { awaitPromise = false, timeoutMs = 30_000 }: { awaitPromise?: boolean; timeoutMs?: number } = {}): Promise<any> {
    const r = await this.cmd(
      "Runtime.evaluate",
      { expression, awaitPromise, returnByValue: true },
      timeoutMs,
    );
    if (r.exceptionDetails) {
      throw new Error(
        `evaluate threw: ${JSON.stringify(r.exceptionDetails).slice(0, 300)}`,
      );
    }
    return r.result?.value;
  }

  async navigate(url: string, settleMs = 3_500): Promise<void> {
    await this.cmd("Page.navigate", { url });
    await new Promise((r) => setTimeout(r, settleMs));
  }

  async setViewport(width: number, height: number): Promise<void> {
    await this.cmd("Emulation.setDeviceMetricsOverride", {
      width,
      height,
      deviceScaleFactor: 1,
      mobile: width < 600,
    });
  }

  async screenshotB64(): Promise<string> {
    const r = await this.cmd("Page.captureScreenshot", { format: "png" }, 60_000);
    return r.data;
  }
}
