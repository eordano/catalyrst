import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, it, vi } from "vitest";
import DeCodeWorkspace from "./DeCodeWorkspace";
import * as folders from "../folder-project";

const fixture = vi.hoisted(() => ({ models: new Map<string, any>(), current: null as any, changed: () => {} }));
vi.mock("./monaco-host", () => {
  const modelFor = (_: unknown, path: string, content: string) => {
    const key = `file:///${path}`;
    if (!fixture.models.has(key)) {
      let value = content;
      fixture.models.set(key, { getValue: () => value, setValue: (next: string) => { value = next; fixture.changed(); } });
    }
    return fixture.models.get(key);
  };
  return { modelFor, languageFor: () => "typescript", loadMonaco: async () => ({ typesLoaded: 1, monaco: {
    Uri: { parse: (value: string) => value }, KeyMod: { CtrlCmd: 1 }, KeyCode: { KeyS: 2 },
    editor: { getModel: (uri: string) => fixture.models.get(uri), create: () => ({
      addCommand: () => {}, dispose: () => {}, onDidChangeModelContent: (fn: () => void) => { fixture.changed = fn; },
      setModel: (model: unknown) => { fixture.current = model; fixture.changed(); },
    }) },
  } }) };
});

beforeEach(() => { fixture.models.clear(); fixture.current = null; });
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("routes a physical folder through a private revision session and retains edits on conflict", async () => {
  const session = { id: "folder", list: async () => ["src/index.ts"], read: vi.fn(async () => "folder original"), write: vi.fn().mockRejectedValue(new Error("File changed outside the editor")) };
  const folder = vi.spyOn(folders, "folderProject").mockReturnValue(session);
  const directory = { name: "scene", async *values() { yield { name: "src", kind: "directory", async *values() { yield { name: "index.ts", kind: "file" }; } }; } } as unknown as FileSystemDirectoryHandle;
  render(<DeCodeWorkspace code={{ getDir: async () => directory }} />);
  await waitFor(() => expect(fixture.current?.getValue()).toBe("folder original"));
  expect(folder).toHaveBeenCalledWith(directory);
  fixture.current.setValue("folder edit");
  await waitFor(() => expect(document.querySelector(".decode-dirty")).not.toBeNull());
  fireEvent.click(screen.getByRole("button", { name: /index.ts/ }));
  expect(session.read).toHaveBeenCalledOnce();
  fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
  expect(await screen.findByRole("alert")).toHaveTextContent("File changed outside");
  expect(session.write).toHaveBeenCalledWith("src/index.ts", "folder edit");
  expect(fixture.current.getValue()).toBe("folder edit");
  expect(document.querySelector(".decode-dirty")).not.toBeNull();
  expect([...fixture.models.keys()][0]).toMatch(/^file:\/\/\/folder\//);
});

it("keeps one private file session through Monaco rerenders", async () => {
  const session = { id: "sdk", list: async () => ["src/index.ts"], read: vi.fn(async () => "original"), write: vi.fn().mockResolvedValue(undefined) };
  const shared = { ...session, read: vi.fn(async () => "wrong session"), createFileSession: vi.fn(() => session) };
  const { rerender } = render(<DeCodeWorkspace code={{ project: shared }} />);
  await waitFor(() => expect(fixture.current?.getValue()).toBe("original"));
  rerender(<DeCodeWorkspace code={{ project: shared }} />);
  expect(shared.createFileSession).toHaveBeenCalledTimes(1);
  expect(shared.read).not.toHaveBeenCalled();
  fixture.current.setValue("edited");
  await waitFor(() => expect(document.querySelector(".decode-dirty")).not.toBeNull());
  fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
  await waitFor(() => expect(session.write).toHaveBeenCalledWith("src/index.ts", "edited"));
});

it("uses the SDK project files and preserves dirty edits across file switches", async () => {
  const project = { id: "http://localhost:8000", list: async () => ["src/index.ts", "scene.json"],
    read: vi.fn(async (path: string) => path === "scene.json" ? "{}" : "original"), write: vi.fn().mockResolvedValue(undefined) };
  render(<DeCodeWorkspace code={{ project }} />);
  await waitFor(() => expect(fixture.current?.getValue()).toBe("original"));
  fireEvent.click(screen.getByRole("button", { name: "index.ts" }));
  await waitFor(() => expect(project.read).toHaveBeenCalledTimes(2));
  fixture.current.setValue("edited");
  await waitFor(() => expect(document.querySelector(".decode-dirty")).not.toBeNull());
  fireEvent.click(screen.getByRole("button", { name: "scene.json" }));
  await waitFor(() => expect(fixture.current.getValue()).toBe("{}"));
  fireEvent.click(screen.getByRole("button", { name: /index.ts/ }));
  await waitFor(() => expect(fixture.current.getValue()).toBe("edited"));
  expect(project.read.mock.calls.filter(([path]) => path === "src/index.ts")).toHaveLength(2);
  fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
  await waitFor(() => expect(project.write).toHaveBeenCalledWith("src/index.ts", "edited"));
});

it("does not clear unsaved edits or claim success when the SDK rejects a conflicting save", async () => {
  const project = { id: "http://localhost:8000", list: async () => ["src/index.ts"], read: async () => "original",
    write: vi.fn().mockRejectedValue(new Error("File changed outside the editor")) };
  render(<DeCodeWorkspace code={{ project }} />);
  await waitFor(() => expect(fixture.current?.getValue()).toBe("original"));
  fixture.current.setValue("edited");
  await waitFor(() => expect(document.querySelector(".decode-dirty")).not.toBeNull());
  fireEvent.click(screen.getByRole("button", { name: /^Save$/ }));
  expect(await screen.findByRole("alert")).toHaveTextContent("File changed outside");
  expect(document.querySelector(".decode-dirty")).not.toBeNull();
});

it("disables editing while a file opens and ignores an older response after switching again", async () => {
  let resolveScene: (value: string) => void = () => {};
  const scene = new Promise<string>((resolve) => { resolveScene = resolve; });
  const project = { id: "http://localhost:8000", list: async () => ["src/index.ts", "scene.json"],
    read: async (path: string) => path === "scene.json" ? scene : "source", write: vi.fn() };
  render(<DeCodeWorkspace code={{ project }} />);
  await waitFor(() => expect(fixture.current?.getValue()).toBe("source"));
  fireEvent.click(screen.getByRole("button", { name: "scene.json" }));
  expect(fixture.current).toBeNull();
  expect(screen.getByRole("button", { name: /^Save$/ })).toBeDisabled();
  fireEvent.click(screen.getByRole("button", { name: "index.ts" }));
  await waitFor(() => expect(fixture.current?.getValue()).toBe("source"));
  await act(async () => { resolveScene("{}"); await scene; });
  expect(fixture.current.getValue()).toBe("source");
  expect(screen.queryByRole("status")).toBeNull();
});
