import { describe, expect, it, vi } from "vitest";
import { generatePrivateKey } from "viem/accounts";

import { deployWorldScene } from "./deploy-scene";
import { createIdentityFromPrivateKey } from "../../auth/identity";
import { hashFile, utf8 } from "../hashing";


function deterministicBytes(len: number): Uint8Array {
  const out = new Uint8Array(len);
  for (let i = 0; i < len; i += 1) out[i] = (i * 31 + 7) % 256;
  return out;
}

describe("deployWorldScene \u{2014} REAL end-to-end publish pipeline (no live wallet/world)", () => {
  it("hashes a multi-block asset, signs, and POSTs a valid multipart deploy", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());

    const sceneJson = utf8(JSON.stringify({ scene: { parcels: ["0,0"] } }));
    const bigBytes = deterministicBytes(300000);

    let capturedUrl = "";
    let capturedForm: FormData | null = null;
    const mockFetch = vi.fn(async (url: string, init: RequestInit) => {
      capturedUrl = String(url);
      capturedForm = init.body as FormData;
      return new Response(JSON.stringify({ creationTimestamp: 1782570659920 }), {
        status: 200,
      });
    }) as unknown as typeof fetch;

    const res = await deployWorldScene(
      identity,
      {
        worldName: "test.dcl.eth",
        files: [
          { file: "scene.json", content: sceneJson },
          { file: "big.glb", content: bigBytes },
        ],
        metadata: { scene: { parcels: ["0,0"] } },
      },
      { fetchImpl: mockFetch, base: "http://x" },
    );

    expect(mockFetch).toHaveBeenCalledTimes(1);
    expect(res).toEqual({ ok: true, status: 200, creationTimestamp: 1782570659920 });

    expect(capturedUrl).toBe("http://x/entities");
    expect(capturedUrl.endsWith("/entities")).toBe(true);

    const form = capturedForm as unknown as FormData;
    expect(form).toBeInstanceOf(FormData);

    const entityId = form.get("entityId") as string;
    expect(entityId).toMatch(/^baf[ky][a-z2-7]+$/);

    const entityBlob = form.get(entityId) as Blob;
    expect(entityBlob).toBeInstanceOf(Blob);
    const entity = JSON.parse(await entityBlob.text()) as {
      id?: string;
      pointers: string[];
      content: { file: string; hash: string }[];
      metadata: { worldConfiguration?: { name?: string } };
    };
    expect(entity.pointers).toEqual(["0,0"]);
    expect(entity.metadata.worldConfiguration?.name).toBe("test.dcl.eth");
    expect(entity.id).toBeUndefined();

    const bigEntry = entity.content.find((c) => c.file === "big.glb");
    expect(bigEntry).toBeDefined();
    expect(bigEntry!.hash).toMatch(/^bafybei/);
    expect(bigEntry!.hash).toBe(await hashFile(bigBytes));

    const sceneEntry = entity.content.find((c) => c.file === "scene.json");
    expect(sceneEntry!.hash).toMatch(/^bafkrei/);
    expect(sceneEntry!.hash).toBe(await hashFile(sceneJson));

    const bigPart = form.get(bigEntry!.hash) as Blob;
    expect(bigPart).toBeInstanceOf(Blob);
    expect(bigPart.size).toBe(bigBytes.byteLength);

    const authTypes: string[] = [];
    for (let i = 0; ; i += 1) {
      const t = form.get(`authChain[${i}][type]`);
      if (t == null) break;
      authTypes.push(String(t));
    }
    expect(authTypes).toContain("SIGNER");
    expect(authTypes).toContain("ECDSA_SIGNED_ENTITY");
    const lastIdx = authTypes.length - 1;
    expect(form.get(`authChain[${lastIdx}][type]`)).toBe("ECDSA_SIGNED_ENTITY");
    expect(form.get(`authChain[${lastIdx}][payload]`)).toBe(entityId);
    expect(String(form.get(`authChain[${lastIdx}][signature]`))).toMatch(/^0x[0-9a-f]+$/i);
  });
});
