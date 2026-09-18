import { beforeEach, expect, it, vi } from "vitest";
import { emptySeed, loadSceneEditorSeed } from "@data/lib/catalyst/creator-hub/scene-editor";
import { loadAssetCatalog } from "@data/lib/catalyst/creator-hub/asset-catalog.server";
import { fetchServerDraft } from "@data/lib/catalyst/creator-hub/scene-drafts-client";
import { storyLoader } from "@core/lib/experiments/story-loader";
import { clientLoader, loader } from "./creator-hub.scene-editor";

vi.mock("@features/stories/creator-hub/scene-editor-place-items/EditorWizard", () => ({ default: () => null }));
vi.mock("@core/lib/experiments/story-loader", () => ({ storyLoader: vi.fn() }));
vi.mock("@data/lib/catalyst/creator-hub/asset-catalog.server", () => ({ loadAssetCatalog: vi.fn() }));
vi.mock("@data/lib/catalyst/creator-hub/scene-drafts-client", () => ({ fetchServerDraft: vi.fn() }));
vi.mock("@data/lib/catalyst/creator-hub/scene-editor", async (original) => ({
  ...await original<object>(), loadSceneEditorSeed: vi.fn(),
}));
vi.mock("@data/lib/fs/handle-store", () => ({
  handleStore: { getMeta: async () => ({ title: "Local", updatedAt: 20 }) },
}));

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((done) => { resolve = done; });
  return { promise, resolve };
}

beforeEach(() => vi.clearAllMocks());

it("starts scene and catalog loading before experiment assignment finishes", async () => {
  const experiment = deferred<Awaited<ReturnType<typeof storyLoader>>>();
  vi.mocked(storyLoader).mockReturnValue(experiment.promise);
  vi.mocked(loadSceneEditorSeed).mockResolvedValue(emptySeed("1,2"));
  vi.mocked(loadAssetCatalog).mockResolvedValue([]);
  const result = loader({ request: new Request("http://127.0.0.1:5158/creator-hub/scene-editor?pointer=1,2", {
    headers: { "x-forwarded-host": "sites.test", "x-forwarded-proto": "https" },
  }) } as never);
  await vi.waitFor(() => expect(loadSceneEditorSeed).toHaveBeenCalledOnce());
  expect(loadAssetCatalog).toHaveBeenCalledOnce();
  experiment.resolve({ sid: "test", assignment: {}, wrap: (value: unknown) => value } as never);
  const payload = await result as unknown as { viewportSrc: string; previewSrc: string };
  expect(payload).toMatchObject({ seed: { scene: { pointer: "1,2" } }, catalog: [] });
  expect(new URL(payload.viewportSrc, "https://sites.test").searchParams.get("editorSession"))
    .toMatch(/^[a-f0-9-]{36}$/i);
  expect(new URL(payload.previewSrc, "https://sites.test").searchParams.get("editorSession")).toBeNull();
  for (const source of [payload.viewportSrc, payload.previewSrc]) {
    expect(new URL(source, "https://sites.test").searchParams.get("winitWorker")).toBe("1");
    expect(new URL(source, "https://sites.test").searchParams.get("portables"))
      .toBe("");
  }
});

it.each([[10, "Local"], [30, "Server"]])("starts the signed draft alongside the route and retains newest-copy precedence (%s)", async (updatedAt, title) => {
  const base = deferred<unknown>();
  vi.mocked(fetchServerDraft).mockResolvedValue({
    id: "draft", version: 1, title: "Server", updatedAt,
    blob: { composite: "{}", title: "Server" },
  });
  const request = new Request("https://sites.test/creator-hub/scene-editor?draft=draft");
  const result = clientLoader({ request, serverLoader: () => base.promise } as never);
  await vi.waitFor(() => expect(fetchServerDraft).toHaveBeenCalledWith("draft", request.signal));
  base.resolve({ seed: emptySeed() });
  expect(await result).toMatchObject({ seed: { scene: { title } } });
});

it("launches a published World scene in its own realm and parcel", async () => {
  vi.mocked(storyLoader).mockResolvedValue({ sid: "test", assignment: {}, wrap: (value: unknown) => value } as never);
  vi.mocked(loadSceneEditorSeed).mockResolvedValue({ ...emptySeed("2,3"), scene: { ...emptySeed("2,3").scene, pointer: "mock.dcl.eth" } });
  vi.mocked(loadAssetCatalog).mockResolvedValue([]);
  const payload = await loader({ request: new Request("https://sites.test/creator-hub/scene-editor?world=mock.dcl.eth&pointer=2,3") } as never) as unknown as { viewportSrc: string; previewSrc: string };
  expect(loadSceneEditorSeed).toHaveBeenCalledWith(expect.objectContaining({ world: "mock.dcl.eth", pointer: "2,3" }));
  for (const source of [payload.viewportSrc, payload.previewSrc]) {
    expect(new URL(source, "https://sites.test").searchParams.get("winitWorker")).toBe("1");
    const query = new URL(source, "https://sites.test").searchParams;
    expect(query.get("realm")).toMatch(/\/world\/mock\.dcl\.eth$/);
    expect(query.get("position")).toBe("2,3");
  }
});
