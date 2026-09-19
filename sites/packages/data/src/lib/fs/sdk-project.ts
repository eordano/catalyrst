export interface SdkProject {
  version: 1;
  name: string;
  scene: Record<string, unknown>;
  compositePath: string | null;
  capabilities: { files: boolean; write: boolean; watch: boolean; dataLayer: boolean; mcp: boolean };
  links: { files: string; file: string; reload: string; preview: string; settings: string; publish: string; storage: string; inspector: string | null; uiDesigner?: string | null; uiDesignerRuntime?: string | null; dataLayer: string | null; mcp: string | null };
}

export interface ProjectFile { path: string; content: string; revision: string }
export interface SdkDebugDescriptor {
  nativeUrl: string;
  multiInstanceUrl: string;
  mobileUrl: string | null;
  mobileQr: string | null;
  eventsUrl: string;
  sessionsUrl: string;
  commandUrl: string;
}
export interface SdkDebugSession {
  id: number;
  sessionId: string | null;
  deviceName: string | null;
  connectedAt: string;
  disconnectedAt: string | null;
  status: "active" | "ended";
  messageCount: number;
}
export type SdkDebugEvent = { type: "sessions"; sessions: SdkDebugSession[] }
  | { type: "entries"; sessionId: number; entries: unknown[]; seq: number }
  | { type: "gap"; missed: number };
type AssistantConversation = { id: string; provider: string; title: string; updatedAt: number; truncated: boolean };
type AssistantSelection = { id: string; name: string };
type AssistantHistoryEvent = AssistantEvent | { type: "user"; text: string; selectedEntities: AssistantSelection[] };
type AssistantEvent = { type: "started"; turnId: string; provider: string; conversationId?: string } | { type: "session"; sessionId: string } | { type: "text"; text: string } | { type: "tool"; name: string; detail: string } | { type: "error"; message: string } | { type: "done"; exitCode: number | null; cancelled: boolean };

async function readJsonLines<T>(body: NonNullable<Response["body"]>, onEvent: (event: T) => void): Promise<void> {
  const reader = body.pipeThrough(new TextDecoderStream()).getReader();
  let pending = "";
  const accept = (line: string) => { if (line.trim()) onEvent(JSON.parse(line) as T); };
  try {
    while (true) {
      const { value, done } = await reader.read();
      if (done) break;
      pending += value;
      let index: number;
      while ((index = pending.indexOf("\n")) >= 0) { accept(pending.slice(0, index)); pending = pending.slice(index + 1); }
    }
    accept(pending);
  } finally { await reader.cancel().catch(() => {}); reader.releaseLock(); }
}

export function sdkProjectUrl(value: string): string {
  const url = new URL(value.trim());
  const loopback = url.hostname === "localhost" || url.hostname === "127.0.0.1" || url.hostname === "[::1]";
  if ((url.protocol !== "https:" && !(url.protocol === "http:" && loopback)) || url.username || url.password || url.search || url.hash) {
    throw new Error("Use the SDK project URL: HTTPS, or HTTP on localhost, without credentials or query parameters.");
  }
  return url.href.replace(/\/+$/, "");
}

export class SdkProjectConnection {
  readonly url: string;
  private revisions = new Map<string, string>();
  private fileOperations = new Map<string, Promise<unknown>>();
  private descriptor: SdkProject | null = null;
  constructor(url: string, private request: typeof fetch = (...args) => fetch(...args)) { this.url = sdkProjectUrl(url); }

  createFileSession(): SdkProjectConnection {
    const session = new SdkProjectConnection(this.url, this.request);
    session.descriptor = this.descriptor;
    return session;
  }

  private async json<T>(path: string, init?: RequestInit): Promise<T> {
    let response: Response;
    try {
      response = await this.request(`${this.url}${path}`, {
        ...init, credentials: "omit", cache: "no-store", signal: init?.signal ?? AbortSignal.timeout(15000),
      });
    } catch (error) {
      throw new Error(`Could not reach the SDK project. Keep dcl-one-sdk start running and allow this site's origin with DCL_ONE_SDK_ALLOWED_ORIGINS. ${error instanceof Error ? error.message : ""}`);
    }
    if (!response.ok) {
      if (response.status === 409 && path.startsWith("/api/project/file?")) throw new Error("This file changed outside the editor. Reopen it before saving your changes.");
      throw new Error(`SDK project request failed (${response.status}). ${await response.text()}`);
    }
    return response.json() as Promise<T>;
  }

  async connect(signal?: AbortSignal): Promise<SdkProject> {
    const project = await this.json<SdkProject>("/api/project", { signal });
    if (project.version !== 1 || typeof project.name !== "string" || !project.scene || !project.capabilities?.files || !project.links) {
      throw new Error("This server does not expose the SDK project bridge. Update dcl-one-sdk and restart the project.");
    }
    this.descriptor = project;
    return project;
  }

  async list(): Promise<string[]> {
    const result = await this.json<{ files: { path: string; size: number }[] }>("/api/project/files");
    return result.files.map((file) => file.path);
  }

  async read(path: string): Promise<string> {
    return this.fileOperation(path, async () => {
      const file = await this.json<ProjectFile>(`/api/project/file?path=${encodeURIComponent(path)}`);
      this.revisions.set(path, file.revision);
      return file.content;
    });
  }

  private fileOperation<T>(path: string, operation: () => Promise<T>): Promise<T> {
    const pending = (this.fileOperations.get(path) ?? Promise.resolve()).catch(() => {}).then(operation);
    this.fileOperations.set(path, pending);
    const clear = () => { if (this.fileOperations.get(path) === pending) this.fileOperations.delete(path); };
    void pending.then(clear, clear);
    return pending;
  }

  async readOnly(path: string): Promise<string> {
    return (await this.json<ProjectFile>(`/api/project/file?path=${encodeURIComponent(path)}`)).content;
  }

  async write(path: string, content: string): Promise<void> {
    return this.fileOperation(path, async () => {
      const file = await this.json<ProjectFile>(`/api/project/file?path=${encodeURIComponent(path)}`, {
        method: "PUT", headers: { "content-type": "application/json" },
        body: JSON.stringify({ content, revision: this.revisions.get(path) ?? null }),
      });
      this.revisions.set(path, file.revision);
    });
  }

  async remove(path: string): Promise<void> {
    return this.fileOperation(path, async () => {
      const revision = this.revisions.get(path);
      if (!revision) throw new Error("Read the source file before removing it.");
      await this.json(`/api/project/file?path=${encodeURIComponent(path)}`, { method: "DELETE", headers: { "if-match": revision } });
      this.revisions.delete(path);
    });
  }

  get uiDesigner(): { url: string; runtimeUrl: string } | undefined {
    const { uiDesigner, uiDesignerRuntime } = this.descriptor?.links ?? {};
    if (!uiDesigner || !uiDesignerRuntime) return undefined;
    const url = new URL(uiDesigner, `${this.url}/`), runtime = new URL(uiDesignerRuntime, `${this.url}/`);
    if (url.origin !== new URL(this.url).origin || runtime.origin !== url.origin) throw new Error("The SDK returned a UI Designer on another server.");
    return { url: url.href, runtimeUrl: runtime.href };
  }

  link(name: "settings" | "publish" | "storage"): string {
    if (!this.descriptor) throw new Error("Connect to the SDK project first.");
    const url = new URL(this.descriptor.links[name], `${this.url}/`);
    if (url.origin !== new URL(this.url).origin) throw new Error("The SDK returned a link to another server.");
    return url.href;
  }

  readonly assets = {
    revision: async (path: string): Promise<string> => {
      const response = await this.request(`${this.url}/api/project/asset?path=${encodeURIComponent(path)}`, { method: "HEAD", credentials: "omit", cache: "no-store", signal: AbortSignal.timeout(15000) });
      const revision = response.headers.get("etag");
      if (!response.ok || !revision) throw new Error(`Could not inspect ${path} (${response.status}).`);
      return revision;
    },
    list: async (): Promise<{ path: string; size: number }[]> => {
      return (await this.json<{ files: { path: string; size: number }[] }>("/api/project/assets")).files;
    },
    read: async (path: string): Promise<{ content: ArrayBuffer; revision: string }> => {
      const response = await this.request(`${this.url}/api/project/asset?path=${encodeURIComponent(path)}`, { credentials: "omit", cache: "no-store", signal: AbortSignal.timeout(30000) });
      if (!response.ok) throw new Error(`Could not read ${path} (${response.status}).`);
      const revision = response.headers.get("etag");
      if (!revision) throw new Error(`The SDK did not provide a revision for ${path}.`);
      return { content: await response.arrayBuffer(), revision };
    },
    write: async (path: string, content: ArrayBuffer, revision?: string): Promise<void> => {
      const response = await this.request(`${this.url}/api/project/asset?path=${encodeURIComponent(path)}`, {
        method: "PUT", credentials: "omit", signal: AbortSignal.timeout(30000),
        headers: { "content-type": "application/octet-stream", ...(revision ? { "if-match": revision } : { "if-none-match": "*" }) }, body: content,
      });
      if (!response.ok) throw new Error(`Could not save ${path} (${response.status}). ${await response.text()}`);
    },
    remove: async (path: string, revision: string): Promise<void> => {
      const response = await this.request(`${this.url}/api/project/asset?path=${encodeURIComponent(path)}`, {
        method: "DELETE", credentials: "omit", signal: AbortSignal.timeout(15000), headers: { "if-match": revision },
      });
      if (!response.ok) throw new Error(`Could not remove ${path} (${response.status}). Refresh the asset list before trying again.`);
    },
  };

  readonly assistant = {
    conversations: () => this.json<{ conversations: AssistantConversation[] }>("/api/project/assistant/conversations"),
    conversation: (id: string) => this.json<AssistantConversation & { events: AssistantHistoryEvent[]; resumable: boolean }>(`/api/project/assistant/conversations/${encodeURIComponent(id)}`),
    deleteConversation: async (id: string): Promise<void> => { await this.json(`/api/project/assistant/conversations/${encodeURIComponent(id)}`, { method: "DELETE" }); },
    providers: () => this.json<{ providers: { id: string; label: string; available: boolean }[]; sceneTools: { available: boolean; url: string | null; paired?: boolean; bridge?: string | null }; busy: boolean }>("/api/project/assistant/providers"),
    turn: async (input: { provider: string; prompt: string; conversationId?: string; selectedEntities?: AssistantSelection[] }, onEvent: (event: AssistantEvent) => void, signal: AbortSignal): Promise<void> => {
      const response = await this.request(`${this.url}/api/project/assistant/turn`, {
        method: "POST", credentials: "omit", signal, headers: { "content-type": "application/json" }, body: JSON.stringify(input),
      });
      if (!response.ok || !response.body) throw new Error(`Assistant request failed (${response.status}): ${await response.text()}`);
      let completed = false;
      await readJsonLines<AssistantEvent>(response.body, (event) => {
        if (event.type === "done") completed = true;
        onEvent(event);
      });
      if (!completed) throw new Error("The assistant disconnected before completing the turn. Check the SDK and try again.");
    },
    cancel: async (turnId: string): Promise<void> => {
      await this.json(`/api/project/assistant/turn/${encodeURIComponent(turnId)}`, { method: "DELETE" });
    },
  };

  readonly debug = {
    descriptor: async (): Promise<SdkDebugDescriptor> => {
      const descriptor = await this.json<SdkDebugDescriptor>("/api/project/debug");
      for (const url of [descriptor.nativeUrl, descriptor.multiInstanceUrl, descriptor.mobileUrl]) {
        if (url !== null && new URL(url).protocol !== "decentraland:") throw new Error("The SDK returned an unsupported device preview link.");
      }
      if (descriptor.mobileQr && !descriptor.mobileQr.startsWith("data:image/svg+xml")) throw new Error("The SDK returned an unsupported mobile QR image.");
      return descriptor;
    },
    sessions: () => this.json<{ sessions: SdkDebugSession[] }>("/api/project/debug/sessions"),
    command: async (sessionId: number, cmd: "pause" | "resume" | "reload_scene"): Promise<void> => {
      const result = await this.json<{ ok: boolean; data?: { error?: string } }>("/api/project/debug/command", {
        method: "POST", headers: { "content-type": "application/json" }, body: JSON.stringify({ sessionId, cmd }),
      });
      if (!result.ok) throw new Error(result.data?.error || "The device did not acknowledge the command.");
    },
    events: async (onEvent: (event: SdkDebugEvent) => void, signal: AbortSignal): Promise<void> => {
      const response = await this.request(`${this.url}/api/project/debug/events`, { credentials: "omit", cache: "no-store", signal });
      if (!response.ok || !response.body) throw new Error(`Device logs could not connect (${response.status}).`);
      await readJsonLines(response.body, onEvent);
      throw new Error("Device logs disconnected. Reconnect to continue.");
    },
  };
}
