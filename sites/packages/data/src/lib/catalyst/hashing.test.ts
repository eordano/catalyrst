import { describe, expect, it } from "vitest";

import {
  base32Lower,
  CHUNK_SIZE_BYTES,
  hashFile,
  hashV1Raw,
  MultiBlockHashUnsupportedError,
  needsMultiBlockHash,
  utf8,
} from "./hashing";

const VECTORS: Array<[string, string]> = [
  ["", "bafkreihdwdcefgh4dqkjv67uzcmw7ojee6xedzdetojuzjevtenxquvyku"],
  ["hello", "bafkreibm6jg3ux5qumhcn2b3flc3tyu6dmlb4xa7u5bf44yegnrjhc4yeq"],
  ["hello world", "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e"],
  ["{}", "bafkreicecnx2gvntm6fbcrvnc336qze6st5u7qq7457igegamd3bzkx7ri"],
];

describe("hashV1Raw", () => {
  it("matches the canonical CIDv1 raw vectors", async () => {
    for (const [input, expected] of VECTORS) {
      expect(await hashV1Raw(utf8(input))).toBe(expected);
    }
  });

  it("always produces a bafkrei\u{2026} (CIDv1 raw, base32) string", async () => {
    const h = await hashV1Raw(utf8("decentraland"));
    expect(h).toMatch(/^bafkrei[a-z2-7]+$/);
    expect(h.length).toBe(59);
  });
});

describe("base32Lower", () => {
  it("encodes with the RFC4648 lowercase, no-pad alphabet", () => {
    expect(base32Lower(new Uint8Array([0x01, 0x55, 0x12, 0x20]))).toMatch(/^[a-z2-7]+$/);
    expect(base32Lower(new Uint8Array([]))).toBe("");
  });
});

describe("hashFile / multi-block guard", () => {
  it("hashes single-block (<=256KiB) input via the raw path", async () => {
    const bytes = utf8("hello world");
    expect(await hashFile(bytes)).toBe(
      "bafkreifzjut3te2nhyekklss27nh3k72ysco7y32koao5eei66wof36n5e",
    );
    expect(needsMultiBlockHash(bytes)).toBe(false);
  });

  it("hashes input exactly at the single-block threshold", async () => {
    const bytes = new Uint8Array(CHUNK_SIZE_BYTES);
    expect(needsMultiBlockHash(bytes)).toBe(false);
    await expect(hashFile(bytes)).resolves.toMatch(/^bafkrei[a-z2-7]+$/);
  });

  it("hashes input that needs the DAG-PB multi-block path (bafybei\u{2026})", async () => {
    const big = new Uint8Array(CHUNK_SIZE_BYTES + 1);
    expect(needsMultiBlockHash(big)).toBe(true);
    const h = await hashFile(big);
    expect(h).toMatch(/^bafybei[a-z2-7]+$/);
    expect(new MultiBlockHashUnsupportedError(big.length)).toBeInstanceOf(Error);
  });
});
