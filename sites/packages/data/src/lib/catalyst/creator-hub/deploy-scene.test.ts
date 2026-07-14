import { describe, expect, it, vi } from "vitest";
import { generatePrivateKey, privateKeyToAccount } from "viem/accounts";
import { recoverMessageAddress } from "viem";

import {
  buildDeployAuthChain,
  buildDeployFormData,
  buildSceneEntity,
  buildSimpleDeployAuthChain,
  deployLandScene,
  deployScene,
  deployWorldScene,
  postDeployment,
} from "./deploy-scene";
import { createIdentityFromPrivateKey } from "../../auth/identity";
import { hashV1Raw, utf8 } from "../hashing";

const META = { main: "bin/index.js", scene: { base: "0,0", parcels: ["0,0"] } };

describe("buildSceneEntity", () => {
  it("hashes each content file (CIDv1) and the entity, building content[]", async () => {
    const files = [
      { file: "scene.json", content: utf8(JSON.stringify(META)) },
      { file: "main.composite", content: utf8('{"version":1,"components":[]}') },
    ];
    const prepared = await buildSceneEntity({ pointers: ["0,0"], files, metadata: META });

    for (const f of files) {
      const entry = prepared.content.find((c) => c.file === f.file)!;
      expect(entry.hash).toBe(await hashV1Raw(f.content));
    }
    expect(prepared.entityId).toBe(await hashV1Raw(prepared.entityFile));
    expect(prepared.entity.id).toBe(prepared.entityId);
    expect(prepared.entity.version).toBe("v3");
    expect(prepared.entity.type).toBe("scene");
    expect(JSON.parse(new TextDecoder().decode(prepared.entityFile)).id).toBeUndefined();
  });

  it("rejects duplicate (case-insensitive) file names", async () => {
    await expect(
      buildSceneEntity({
        pointers: ["0,0"],
        files: [
          { file: "Scene.json", content: utf8("a") },
          { file: "scene.json", content: utf8("b") },
        ],
        metadata: META,
      }),
    ).rejects.toThrow(/duplicate/i);
  });

  it("requires at least one pointer", async () => {
    await expect(
      buildSceneEntity({ pointers: [], files: [], metadata: META }),
    ).rejects.toThrow(/pointer/i);
  });

  it("references pre-uploaded hashes without re-uploading bytes", async () => {
    const prepared = await buildSceneEntity({
      pointers: ["0,0"],
      files: [{ file: "scene.json", content: utf8("{}") }],
      metadata: META,
      referencedHashes: [
        { file: "model.glb", hash: "bafybeibigmodelhashxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx" },
      ],
    });
    expect(prepared.content.find((c) => c.file === "model.glb")?.hash).toMatch(/^bafy/);
    expect(prepared.files.some((f) => f.file === "model.glb")).toBe(false);
  });
});

describe("deployment auth chains", () => {
  it("appends an ECDSA_SIGNED_ENTITY link that recovers to the EPHEMERAL key", async () => {
    const pk = generatePrivateKey();
    const identity = await createIdentityFromPrivateKey(pk);
    const entityId = "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku";

    const chain = await buildDeployAuthChain(identity, entityId);
    expect(chain).toHaveLength(3);
    const last = chain[2];
    expect(last.type).toBe("ECDSA_SIGNED_ENTITY");
    expect(last.payload).toBe(entityId);

    const recovered = await recoverMessageAddress({ message: entityId, signature: last.signature as `0x${string}` });
    expect(recovered.toLowerCase()).toBe(identity.ephemeral.address.toLowerCase());
  });

  it("simple chain: SIGNER + ECDSA_SIGNED_ENTITY signed directly by the wallet key", async () => {
    const pk = generatePrivateKey();
    const addr = privateKeyToAccount(pk).address.toLowerCase();
    const entityId = "bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq";

    const chain = await buildSimpleDeployAuthChain(pk, entityId);
    expect(chain.map((l) => l.type)).toEqual(["SIGNER", "ECDSA_SIGNED_ENTITY"]);
    expect(chain[0].payload).toBe(addr);
    expect(chain[0].signature).toBe("");
    const recovered = await recoverMessageAddress({ message: entityId, signature: chain[1].signature as `0x${string}` });
    expect(recovered.toLowerCase()).toBe(addr);
  });
});

describe("buildDeployFormData (multipart field encoding)", () => {
  it("emits entityId, flattened authChain[i][...], and one file part per hash", async () => {
    const prepared = await buildSceneEntity({
      pointers: ["0,0"],
      files: [{ file: "scene.json", content: utf8(JSON.stringify(META)) }],
      metadata: META,
    });
    const chain = await buildSimpleDeployAuthChain(generatePrivateKey(), prepared.entityId);
    const form = buildDeployFormData(prepared, chain);

    expect(form.get("entityId")).toBe(prepared.entityId);
    expect(form.get("authChain[0][type]")).toBe("SIGNER");
    expect(form.get("authChain[1][type]")).toBe("ECDSA_SIGNED_ENTITY");
    expect(form.get("authChain[1][payload]")).toBe(prepared.entityId);
    expect(form.get(prepared.entityId)).toBeInstanceOf(Blob);
    const sceneHash = prepared.content.find((c) => c.file === "scene.json")!.hash;
    expect(form.get(sceneHash)).toBeInstanceOf(Blob);
  });
});

describe("deployScene (HTTP contract, mocked transport)", () => {
  const prep = async () =>
    buildSceneEntity({
      pointers: ["0,0"],
      files: [{ file: "scene.json", content: utf8(JSON.stringify(META)) }],
      metadata: META,
    });

  it("reports a successful deploy (200 + creationTimestamp)", async () => {
    const prepared = await prep();
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    const fetchImpl = vi.fn(async (url: string, init: RequestInit) => {
      expect(String(url)).toMatch(/\/content\/entities$/);
      expect(init.method).toBe("POST");
      expect(init.body).toBeInstanceOf(FormData);
      return new Response(JSON.stringify({ creationTimestamp: 1782570659920 }), { status: 200 });
    }) as unknown as typeof fetch;

    const res = await deployScene(identity, prepared, { base: "http://cat", fetchImpl });
    expect(res).toEqual({ ok: true, status: 200, creationTimestamp: 1782570659920 });
  });

  it("reports rejection (e.g. ownership) without throwing", async () => {
    const prepared = await prep();
    const chain = await buildSimpleDeployAuthChain(generatePrivateKey(), prepared.entityId);
    const fetchImpl = vi.fn(async () =>
      new Response(
        JSON.stringify({ errors: ["The provided Eth Address does not have access to the following parcel: (0,0)"] }),
        { status: 400 },
      ),
    ) as unknown as typeof fetch;

    const res = await postDeployment(prepared, chain, { base: "http://cat", fetchImpl });
    expect(res.ok).toBe(false);
    if (!res.ok) {
      expect(res.status).toBe(400);
      expect(res.errors[0]).toMatch(/does not have access/);
    }
  });

  it("reports a network failure as a non-throwing error result", async () => {
    const prepared = await prep();
    const chain = await buildSimpleDeployAuthChain(generatePrivateKey(), prepared.entityId);
    const fetchImpl = vi.fn(async () => {
      throw new Error("connection refused");
    }) as unknown as typeof fetch;
    const res = await postDeployment(prepared, chain, { base: "http://cat", fetchImpl });
    expect(res).toEqual({ ok: false, status: 0, errors: ["connection refused"] });
  });

  it("postDeployment targets a custom path (worlds /entities, no /content prefix)", async () => {
    const prepared = await prep();
    const chain = await buildSimpleDeployAuthChain(generatePrivateKey(), prepared.entityId);
    const fetchImpl = vi.fn(async (url: string) => {
      expect(String(url)).toBe("http://worlds/entities");
      return new Response(JSON.stringify({ creationTimestamp: 1 }), { status: 200 });
    }) as unknown as typeof fetch;
    const res = await postDeployment(prepared, chain, {
      base: "http://worlds",
      path: "/entities",
      fetchImpl,
    });
    expect(res.ok).toBe(true);
  });
});

describe("deployWorldScene (worlds-content-server contract)", () => {
  const WORLD_META = {
    main: "bin/index.js",
    scene: { base: "20,24", parcels: ["20,24", "20,25"] },
  };

  it("POSTs to <worldsBase>/entities and stamps worldConfiguration.name = the world name", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    let capturedEntity: Record<string, unknown> | null = null;
    const fetchImpl = vi.fn(async (url: string, init: RequestInit) => {
      expect(String(url)).toBe("http://worlds/entities");
      const form = init.body as FormData;
      const entityId = form.get("entityId") as string;
      const entityBlob = form.get(entityId) as Blob;
      capturedEntity = JSON.parse(await entityBlob.text());
      return new Response(JSON.stringify({ creationTimestamp: 42 }), { status: 200 });
    }) as unknown as typeof fetch;

    const res = await deployWorldScene(
      identity,
      { worldName: "my-name.dcl.eth", files: [{ file: "scene.json", content: utf8(JSON.stringify(WORLD_META)) }], metadata: WORLD_META },
      { base: "http://worlds", fetchImpl },
    );

    expect(res).toEqual({ ok: true, status: 200, creationTimestamp: 42 });
    expect(capturedEntity).not.toBeNull();
    const entity = capturedEntity as unknown as Record<string, unknown>;
    expect(entity.pointers).toEqual(["20,24", "20,25"]);
    expect(entity.pointers).not.toContain("my-name.dcl.eth");
    const meta = entity.metadata as { worldConfiguration?: { name?: string } };
    expect(meta.worldConfiguration?.name).toBe("my-name.dcl.eth");
  });

  it("fails closed when the world name is missing/blank (never signs/POSTs)", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    const fetchImpl = vi.fn() as unknown as typeof fetch;
    await expect(
      deployWorldScene(
        identity,
        { worldName: "  ", files: [], metadata: WORLD_META },
        { base: "http://worlds", fetchImpl },
      ),
    ).rejects.toThrow(/world name/i);
    expect(fetchImpl).not.toHaveBeenCalled();
  });

  it("fails closed when there are no scene parcels to point at", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    const fetchImpl = vi.fn() as unknown as typeof fetch;
    await expect(
      deployWorldScene(
        identity,
        { worldName: "my-name.dcl.eth", files: [], metadata: { scene: { parcels: [] } } },
        { base: "http://worlds", fetchImpl },
      ),
    ).rejects.toThrow(/parcels/i);
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});

describe("deployLandScene (catalyst content contract)", () => {
  const LAND_META = {
    main: "bin/index.js",
    scene: { base: "52,-52", parcels: ["52,-52", "53,-52"] },
    worldConfiguration: { name: "stale-world.dcl.eth" },
  };

  it("POSTs to <catalyst>/content/entities with parcel pointers and NO worldConfiguration", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    let capturedEntity: Record<string, unknown> | null = null;
    const fetchImpl = vi.fn(async (url: string, init: RequestInit) => {
      expect(String(url)).toBe("http://cat/content/entities");
      const form = init.body as FormData;
      const entityId = form.get("entityId") as string;
      const entityBlob = form.get(entityId) as Blob;
      capturedEntity = JSON.parse(await entityBlob.text());
      return new Response(JSON.stringify({ creationTimestamp: 7 }), { status: 200 });
    }) as unknown as typeof fetch;

    const res = await deployLandScene(
      identity,
      {
        files: [{ file: "scene.json", content: utf8(JSON.stringify(LAND_META)) }],
        metadata: LAND_META,
      },
      { base: "http://cat", fetchImpl },
    );

    expect(res).toEqual({ ok: true, status: 200, creationTimestamp: 7 });
    const entity = capturedEntity as unknown as Record<string, unknown>;
    expect(entity.pointers).toEqual(["52,-52", "53,-52"]);
    const meta = entity.metadata as Record<string, unknown>;
    expect(meta.worldConfiguration).toBeUndefined();
    expect(meta.scene).toEqual(LAND_META.scene);
  });

  it("fails closed when there are no scene parcels to point at", async () => {
    const identity = await createIdentityFromPrivateKey(generatePrivateKey());
    const fetchImpl = vi.fn() as unknown as typeof fetch;
    await expect(
      deployLandScene(
        identity,
        { files: [], metadata: { scene: { parcels: [] } } },
        { base: "http://cat", fetchImpl },
      ),
    ).rejects.toThrow(/parcels/i);
    expect(fetchImpl).not.toHaveBeenCalled();
  });
});
