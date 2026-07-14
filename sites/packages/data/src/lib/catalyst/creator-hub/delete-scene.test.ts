import { describe, expect, it, vi } from "vitest";
import { generatePrivateKey } from "viem/accounts";
import { recoverMessageAddress } from "viem";

import {
  buildSceneDeletion,
  buildTombstoneSceneJson,
  deleteScene,
  resolveActiveScene,
} from "./delete-scene";
import { createIdentityFromPrivateKey } from "../../auth/identity";
import { hashV1Raw } from "../hashing";

describe("buildTombstoneSceneJson", () => {
  it("is a valid empty scene with no worldConfiguration / navmapThumbnail", () => {
    const meta = buildTombstoneSceneJson(["1,2", "1,3"], "1,2", 123);
    expect(meta.scene).toEqual({ base: "1,2", parcels: ["1,2", "1,3"] });
    expect(meta).not.toHaveProperty("worldConfiguration");
    expect((meta.display as Record<string, unknown>).navmapThumbnail).toBeUndefined();
    expect(meta.dclDeleted).toBe(true);
  });
});

describe("buildSceneDeletion", () => {
  it("builds a tombstone deployment hashed to a CIDv1 entityId over the pointers", async () => {
    const prepared = await buildSceneDeletion({ pointers: ["12,34"], timestamp: 1 });
    expect(prepared.entity.type).toBe("scene");
    expect(prepared.entity.pointers).toEqual(["12,34"]);
    expect(prepared.entityId).toBe(await hashV1Raw(prepared.entityFile));
    expect(prepared.files.some((f) => f.file === "scene.json")).toBe(true);
    expect(JSON.parse(new TextDecoder().decode(prepared.entityFile)).id).toBeUndefined();
  });

  it("rejects non-parcel pointers (won't tombstone arbitrary pointers)", async () => {
    await expect(
      buildSceneDeletion({ pointers: ["0xdeadbeef"] }),
    ).rejects.toThrow(/not a parcel/i);
  });

  it("requires at least one pointer", async () => {
    await expect(buildSceneDeletion({ pointers: [] })).rejects.toThrow(/pointer/i);
  });
});

describe("resolveActiveScene", () => {
  it("returns the active scene's id + pointers for a parcel", async () => {
    const fetchImpl = vi.fn(async (url: string) => {
      expect(String(url)).toContain("/content/entities/scene?pointer=5%2C6");
      return new Response(
        JSON.stringify([{ id: "bafkreitest", pointers: ["5,6", "5,7"], timestamp: 9 }]),
        { status: 200 },
      );
    }) as unknown as typeof fetch;
    const live = await resolveActiveScene("5,6", { base: "http://cat", fetchImpl });
    expect(live).toEqual({ id: "bafkreitest", pointers: ["5,6", "5,7"], timestamp: 9 });
  });

  it("returns null when no scene is active at the pointer", async () => {
    const fetchImpl = vi.fn(async () =>
      new Response(JSON.stringify([]), { status: 200 }),
    ) as unknown as typeof fetch;
    expect(await resolveActiveScene("9,9", { base: "http://cat", fetchImpl })).toBeNull();
  });
});

describe("deleteScene (HTTP contract, mocked transport)", () => {
  it("signs an owner override and reports a real success", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    let posted: { url: string; body: FormData } | null = null;
    const fetchImpl = vi.fn(async (url: string, init: RequestInit) => {
      posted = { url: String(url), body: init.body as FormData };
      expect(init.method).toBe("POST");
      return new Response(JSON.stringify({ creationTimestamp: 1782570659920 }), {
        status: 200,
      });
    }) as unknown as typeof fetch;

    const res = await deleteScene(
      identity,
      { pointers: ["100,100"] },
      { base: "http://cat", fetchImpl, expectedOwner: identity.signer },
    );

    expect(res.ok).toBe(true);
    if (res.ok) {
      expect(res.status).toBe(200);
      expect(res.tombstoneId).toMatch(/^bafkrei/);
      expect(res.overrode).toEqual(["100,100"]);
    }
    expect(posted!.url).toMatch(/\/content\/entities$/);
    const entityId = posted!.body.get("entityId") as string;
    const sigLink = posted!.body.get("authChain[2][signature]") as string;
    const recovered = await recoverMessageAddress({
      message: entityId,
      signature: sigLink as `0x${string}`,
    });
    expect(recovered.toLowerCase()).toBe(identity.ephemeral.address.toLowerCase());
  });

  it("refuses to delete when the connected wallet is not the owner", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    const fetchImpl = vi.fn() as unknown as typeof fetch;
    const res = await deleteScene(
      identity,
      { pointers: ["1,1"] },
      { base: "http://cat", fetchImpl, expectedOwner: "0xsomeoneelse" },
    );
    expect(res.ok).toBe(false);
    if (!res.ok) expect(res.errors[0]).toMatch(/not the scene owner/i);
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it("reports a catalyst ownership rejection without throwing", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({
          errors: ["The provided Eth Address does not have access to the following parcel: (1,1)"],
        }),
        { status: 400 },
      ),
    ) as unknown as typeof fetch;
    const res = await deleteScene(
      identity,
      { pointers: ["1,1"] },
      { base: "http://cat", fetchImpl, expectedOwner: identity.signer },
    );
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.status).toBe(400);
      expect(res.errors[0]).toMatch(/does not have access/);
    }
  });
});
