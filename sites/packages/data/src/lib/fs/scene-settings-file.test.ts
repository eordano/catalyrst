import { describe, expect, it, vi } from "vitest";
import { openSceneSettingsFile } from "./scene-settings-file";

describe("scene settings persistence", () => {
  it("uses one private settings session and non-editing conflict probes", async () => {
    const session = { read: vi.fn(async () => "{}"), readOnly: vi.fn(async () => "{}"), write: vi.fn(async () => {}) };
    const shared = { read: vi.fn(), write: vi.fn(), createFileSession: vi.fn(() => session) };
    const file = await openSceneSettingsFile({ project: shared });
    await file.save('{"changed":true}');
    expect(shared.createFileSession).toHaveBeenCalledOnce();
    expect(shared.read).not.toHaveBeenCalled();
    expect(session.read).toHaveBeenCalledOnce();
    expect(session.readOnly).toHaveBeenCalledOnce();
    expect(session.write).toHaveBeenCalledWith("scene.json", '{"changed":true}');
  });
  it("saves to the shared SDK file adapter and rejects external changes", async () => {
    let content = '{"display":{"title":"Before"}}';
    const write = vi.fn(async (_path: string, next: string) => { content = next; });
    const file = await openSceneSettingsFile({ project: { read: async () => content, write } });
    await file.save('{"display":{"title":"After"}}');
    expect(write).toHaveBeenCalledWith("scene.json", '{"display":{"title":"After"}}');
    content = '{"external":true}';
    await expect(file.save("{}")).rejects.toThrow(/changed outside/);
    expect(write).toHaveBeenCalledTimes(1);
  });
  it("uses browser code files and detects failed draft storage", async () => {
    const persisted: Record<string, string> = {};
    const files = { virtualFiles: [{ path: "scene.json", text: "{}" }], hydrate: async () => persisted, persist: async (path: string, text: string) => { persisted[path] = text; } };
    const file = await openSceneSettingsFile(files);
    expect(file.destination).toBe("Browser draft");
    await file.save('{"changed":true}');
    expect(persisted["scene.json"]).toBe('{"changed":true}');
    const failed = await openSceneSettingsFile({ ...files, persist: async () => {} });
    await expect(failed.save("{}")).rejects.toThrow(/could not save/);
  });
  it("writes the existing folder file only after checking for concurrent changes", async () => {
    let content = "{}";
    const write = vi.fn(async (text: string) => { content = text; });
    const close = vi.fn(async () => {});
    const handle = { getFile: async () => ({ text: async () => content }), createWritable: async () => ({ write, close }) };
    const dir = { getFileHandle: vi.fn(async () => handle) } as unknown as FileSystemDirectoryHandle;
    const file = await openSceneSettingsFile({ getDir: async () => dir });
    expect(file.destination).toBe("Project folder");
    await file.save('{"folder":true}');
    expect(close).toHaveBeenCalledOnce();
    content = '{"someoneElse":true}';
    await expect(file.save("{}")).rejects.toThrow(/changed outside/);
    expect(write).toHaveBeenCalledOnce();
  });
});
