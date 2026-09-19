import { describe, expect, it, vi } from "vitest";
import { designerStorage } from "./ui-designer-bridge";
function fixture() {
  const files = new Map<string, string>([["src/ui/Hud.tsx", "// caf\u00e9 \u{1f98a}\nexport const Hud = () => <Label value='score' />\n"], ["scene.json", "{}"]]);
  const project = { id: "sdk", list: async () => [...files.keys()], read: async (path: string) => files.get(path)!, write: vi.fn(async (path: string, text: string) => { files.set(path, text); }), remove: vi.fn(async (path: string) => { files.delete(path); }) };
  return { files, project, storage: designerStorage(project) };
}
describe("upstream UI Designer project storage", () => {
  it("owns an isolated file session and probes stat without advancing its edit baseline", async () => {
    const { project } = fixture();
    const read = vi.fn(project.read), readOnly = vi.fn(project.read);
    const session = { ...project, read, readOnly };
    const createFileSession = vi.fn(() => session);
    const storage = designerStorage({ ...project, createFileSession });
    await storage.read_file!({ path: "src/ui/Hud.tsx" });
    await storage.stat!({ path: "src/ui/Hud.tsx" });
    await storage.write_file!({ path: "src/ui/Hud.tsx", content: new TextEncoder().encode("updated") });
    expect(createFileSession).toHaveBeenCalledOnce();
    expect(read).toHaveBeenCalledOnce();
    expect(readOnly).toHaveBeenCalledOnce();
  });
  it("round trips UTF-8 source and opaque expressions without reprinting", async () => {
    const { files, storage } = fixture();
    const source = new TextDecoder().decode(await storage.read_file!({ path: "src/ui/Hud.tsx" }) as Uint8Array);
    const edited = source.replace("'score'", "'points'") + "// opaque\nconst items = data.map(item => <Label value={item.title} />);\n";
    await storage.write_file!({ path: "src/ui/Hud.tsx", content: new TextEncoder().encode(edited) });
    expect(files.get("src/ui/Hud.tsx")).toBe(edited);
    expect(await storage.stat!({ path: "src/ui/Hud.tsx" })).toEqual({ size: new TextEncoder().encode(edited).byteLength });
  });
  it("supports upstream root file create, list, rename and removal", async () => {
    const { storage } = fixture();
    expect(await storage.list!({ path: "src" })).toEqual([{ name: "ui", isDirectory: true }]);
    const source = await storage.read_file!({ path: "src/ui/Hud.tsx" });
    await storage.write_file!({ path: "src/ui/Score.tsx", content: source });
    await storage.delete!({ path: "src/ui/Hud.tsx" });
    expect(await storage.exists!({ path: "src/ui/Hud.tsx" })).toBe(false);
    expect(await storage.list!({ path: "src/ui" })).toEqual([{ name: "Score.tsx", isDirectory: false }]);
    await expect(storage.rmdir!({ path: "src/ui" })).rejects.toThrow("not empty");
  });
  it("propagates file conflicts and rejects paths outside source before writing", async () => {
    const { storage, project } = fixture();
    project.write.mockRejectedValueOnce(new Error("changed outside the editor"));
    await expect(storage.write_file!({ path: "src/ui/Hud.tsx", content: new Uint8Array() })).rejects.toThrow("changed outside");
    await expect(storage.write_file!({ path: "../src/ui/Hud.tsx", content: new Uint8Array() })).rejects.toThrow("Invalid asset path");
    await expect(storage.write_file!({ path: "scene.json", content: new Uint8Array() })).rejects.toThrow("source files only");
    expect(project.write).toHaveBeenCalledTimes(1);
  });
});

it("accepts storage messages only from the bound frame and origin and detaches listeners", async () => {
  const { attachDesignerBridge } = await import("./ui-designer-bridge");
  const { project } = fixture();
  const frame = document.createElement("iframe");
  frame.src = "https://sdk.test/inspector/";
  document.body.append(frame);
  const delivered = vi.fn();
  class Transport { emit = delivered; send() {} dispose() {} }
  class RPC { handle() {} dispose() {} }
  const detach = attachDesignerBridge(frame, project, { Transport, RPC, parse: async () => ({}) }, vi.fn());
  const dispatch = (source: Window | null, origin: string) => window.dispatchEvent(new MessageEvent("message", { source, origin, data: { id: "IframeStorage" } }));
  dispatch(window, "https://sdk.test");
  dispatch(frame.contentWindow, "https://evil.test");
  expect(delivered).not.toHaveBeenCalled();
  dispatch(frame.contentWindow, "https://sdk.test");
  expect(delivered).toHaveBeenCalledOnce();
  detach();
  dispatch(frame.contentWindow, "https://sdk.test");
  expect(delivered).toHaveBeenCalledOnce();
  frame.remove();
});

it("leaves optional file probes to upstream while reporting rejected writes", async () => {
  const { attachDesignerBridge } = await import("./ui-designer-bridge");
  const { project } = fixture();
  const frame = document.createElement("iframe");
  frame.src = "https://sdk.test/inspector/";
  const handlers = new Map<string, (params: Record<string, unknown>) => Promise<unknown>>();
  class Transport { emit() {} send() {} dispose() {} }
  class RPC {
    constructor(private id: string) {}
    handle(method: string, handler: (params: Record<string, unknown>) => Promise<unknown>) { handlers.set(this.id + "/" + method, handler); }
    dispose() {}
  }
  const report = vi.fn();
  const detach = attachDesignerBridge(frame, project, { Transport, RPC, parse: async () => ({}) }, report);
  await expect(handlers.get("IframeStorage/read_file")!({ path: "src/ui/index.tsx" })).rejects.toThrow("does not exist");
  expect(report).not.toHaveBeenCalled();
  project.write.mockRejectedValueOnce(new Error("File changed externally"));
  await expect(handlers.get("IframeStorage/write_file")!({ path: "src/ui/Hud.tsx", content: new Uint8Array() })).rejects.toThrow("File changed externally");
  expect(report).toHaveBeenLastCalledWith("File changed externally");
  await handlers.get("IframeStorage/write_file")!({ path: "src/ui/Hud.tsx", content: new Uint8Array() });
  expect(report).toHaveBeenLastCalledWith(null);
  detach();
});
