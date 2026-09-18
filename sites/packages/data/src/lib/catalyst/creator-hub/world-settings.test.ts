import { beforeEach, describe, expect, it, vi } from "vitest";
import { loadWorldSettings, saveWorldSettings, settingsForm } from "./world-settings";
import { signedFetch } from "../../auth/signer";
import { getJSON } from "../client";
import type { AuthIdentity } from "../../auth/types";

vi.mock("../../auth/signer", () => ({ signedFetch: vi.fn() }));
vi.mock("../client", async (original) => ({ ...await original<object>(), getJSON: vi.fn() }));
const identity = { signer: "0xmock" } as AuthIdentity;
const wire = { title: "My World", description: "A World description", categories: ["art"], spawn_coordinates: "0,0", skybox_time: null, single_player: false, show_in_places: true, thumbnail_hash: "image-hash" };
beforeEach(() => vi.resetAllMocks());

describe("live World settings", () => {
  it("loads persisted fields and the served thumbnail", async () => {
    vi.mocked(getJSON).mockResolvedValue(wire);
    const value = await loadWorldSettings("mock.dcl.eth", { base: "https://worlds.test" });
    expect(getJSON).toHaveBeenCalledWith("/world/mock.dcl.eth/settings", expect.objectContaining({ base: "https://worlds.test" }));
    expect(value).toMatchObject({ title: "My World", categories: ["art"], thumbnailUrl: "https://worlds.test/contents/image-hash", skyboxTime: null, showInPlaces: true });
  });

  it("signs multipart changes and uses the stored response rather than optimistic values", async () => {
    vi.mocked(signedFetch).mockResolvedValue(Response.json({ settings: { ...wire, title: "Stored title", categories: [], single_player: true } }));
    const thumbnail = new File([new Uint8Array([137, 80, 78, 71])], "image.png", { type: "image/png" });
    const value = await saveWorldSettings("mock.dcl.eth", { title: "Draft title", categories: [], skyboxTime: null, singlePlayer: true }, { identity, thumbnail, base: "https://worlds.test" });
    const [signer, url, request] = vi.mocked(signedFetch).mock.calls[0];
    expect(signer).toBe(identity);
    expect(url).toBe("https://worlds.test/world/mock.dcl.eth/settings");
    expect(request?.method).toBe("PUT");
    const body = request?.body as FormData;
    expect(body.getAll("categories")).toEqual(["null"]);
    expect(body.get("skybox_time")).toBe("null");
    expect(body.get("single_player")).toBe("true");
    expect(body.has("description")).toBe(false);
    expect((body.get("thumbnail") as File).name).toBe("image.png");
    expect(value.title).toBe("Stored title");
  });

  it("preserves server authorization and validation errors", async () => {
    vi.mocked(signedFetch).mockResolvedValue(Response.json({ message: "Not the owner" }, { status: 403 }));
    await expect(saveWorldSettings("mock.dcl.eth", { title: "My World" }, { identity })).rejects.toThrow("Not the owner");
  });

  it.each([{ title: "" }, { description: "no" }, { spawnCoordinates: "0," }, { spawnCoordinates: "151,0" }, { skyboxTime: NaN }, { skyboxTime: 86400 }])("rejects invalid form data before requesting a save: %j", (changes) => {
    expect(() => settingsForm(changes)).toThrow();
    expect(signedFetch).not.toHaveBeenCalled();
  });

  it("rejects oversized and unsupported thumbnails", () => {
    expect(() => settingsForm({}, new File([new Uint8Array(1024 * 1024 + 1)], "large.png", { type: "image/png" }))).toThrow("1 MB");
    expect(() => settingsForm({}, new File(["<svg/>"], "image.svg", { type: "image/svg+xml" }))).toThrow("PNG");
  });
});
