import type { DeWorkspaceCode } from "./types";
import { safeAssetPath } from "./project-assets";

type Project = NonNullable<DeWorkspaceCode["project"]>;
type Handler = (params: Record<string, unknown>) => Promise<unknown>;
interface Transport { emit(type: string, message: unknown): void; send(message: unknown): void; dispose(): void }
interface Rpc { handle(method: string, handler: Handler): void; dispose(): void }
export interface DesignerRuntime {
  parse(filename: string, source: string): Promise<unknown>;
  RPC: new (id: string, transport: Transport) => Rpc;
  Transport: new () => Transport;
}
function pathParam(value: unknown, allowRoot = false): string {
  if (typeof value !== "string") throw new Error("A project path is required.");
  if (allowRoot && (value === "" || value === ".")) return "";
  return safeAssetPath(value.replace(/\/$/, ""));
}
function bytes(value: unknown): Uint8Array {
  if (ArrayBuffer.isView(value)) return new Uint8Array(value.buffer, value.byteOffset, value.byteLength);
  if (value instanceof ArrayBuffer) return new Uint8Array(value);
  if (value && typeof value === "object" && "type" in value && value.type === "Buffer" && "data" in value && Array.isArray(value.data)) return new Uint8Array(value.data);
  throw new Error("Expected UTF-8 file contents.");
}
export function designerStorage(sharedProject: Project): Record<string, Handler> {
  const project = sharedProject.createFileSession?.() ?? sharedProject;
  const files = async () => [...new Set([...(await project.list()), ...((await project.assets?.list()) ?? []).map(file => file.path)])];
  const read = async (path: string, readOnly = false) => {
    if ((await project.list()).includes(path)) return new TextEncoder().encode(await (readOnly && project.readOnly ? project.readOnly(path) : project.read(path)));
    if (project.assets && (await project.assets.list()).some(file => file.path === path)) return new Uint8Array((await project.assets.read(path)).content);
    throw new Error(`File does not exist: ${path}`);
  };
  return {
    read_file: async ({ path }) => read(pathParam(path)),
    write_file: async ({ path, content }) => {
      const target = pathParam(path);
      if (!/^src\/.+\.(tsx?|jsx?)$/.test(target)) throw new Error("The UI Designer can write scene source files only.");
      await project.write(target, new TextDecoder("utf-8", { fatal: true }).decode(bytes(content)));
    },
    exists: async ({ path }) => { const target = pathParam(path, true); return !target || (await files()).some(file => file === target || file.startsWith(target + "/")); },
    list: async ({ path }) => {
      const target = pathParam(path, true), prefix = target ? target + "/" : "";
      const entries = new Map<string, boolean>();
      for (const file of await files()) if (file.startsWith(prefix)) {
        const [name, child] = file.slice(prefix.length).split("/");
        if (name) entries.set(name, entries.get(name) || child !== undefined);
      }
      return [...entries].map(([name, isDirectory]) => ({ name, isDirectory }));
    },
    stat: async ({ path }) => ({ size: (await read(pathParam(path), true)).byteLength }),
    delete: async ({ path }) => {
      const target = pathParam(path);
      if (!/^src\/.+\.(tsx?|jsx?)$/.test(target)) throw new Error("The UI Designer can remove scene source files only.");
      if (!project.remove) throw new Error("This project does not support removing source files.");
      await project.remove(target);
    },
    rmdir: async ({ path }) => {
      const target = pathParam(path);
      if ((await files()).some(file => file.startsWith(target + "/"))) throw new Error("The directory is not empty.");
    },
  };
}
export function attachDesignerBridge(iframe: HTMLIFrameElement, project: Project, runtime: DesignerRuntime, onError: (message: string | null) => void): () => void {
  const origin = new URL(iframe.src).origin;
  class FrameTransport extends runtime.Transport {
    private receive = (event: MessageEvent) => {
      if (event.source === iframe.contentWindow && event.origin === origin) this.emit("message", event.data);
    };
    constructor() { super(); window.addEventListener("message", this.receive); }
    override send(message: unknown) { iframe.contentWindow?.postMessage(message, origin); }
    override dispose() { window.removeEventListener("message", this.receive); }
  }
  const transport = new FrameTransport();
  const storage = new runtime.RPC("IframeStorage", transport);
  const parser = new runtime.RPC("CodeParser", transport);
  const checked = (handler: Handler): Handler => async params => {
    try { const result = await handler(params); onError(null); return result; } catch (error) { onError(error instanceof Error ? error.message : String(error)); throw error; }
  };
  for (const [method, handler] of Object.entries(designerStorage(project))) storage.handle(method, ["write_file", "delete", "rmdir"].includes(method) ? checked(handler) : handler);
  parser.handle("parse", checked(async ({ filename, source }) => {
    if (typeof filename !== "string" || typeof source !== "string") throw new Error("Invalid parser request.");
    return runtime.parse(filename, source);
  }));
  return () => { storage.dispose(); parser.dispose(); transport.dispose(); };
}
