import { describe, expect, it } from "vitest";

import { crc32, makeZip } from "./zip";

const enc = new TextEncoder();

describe("crc32 \u{2014} known IEEE vectors", () => {
  it("matches reference CRC-32 values", () => {
    expect(crc32(enc.encode(""))).toBe(0x00000000);
    expect(crc32(enc.encode("hello"))).toBe(0x3610a686);
    expect(crc32(enc.encode("The quick brown fox jumps over the lazy dog"))).toBe(0x414fa339);
  });
});

describe("makeZip \u{2014} a valid STORE zip", () => {
  const zip = makeZip([
    { path: "proj/scene.json", text: '{"name":"x"}' },
    { path: "proj/src/index.ts", text: "export function main() {}" },
  ]);

  it("starts with the local-file-header signature PK\\x03\\x04", () => {
    expect([...zip.slice(0, 4)]).toEqual([0x50, 0x4b, 0x03, 0x04]);
  });

  it("contains an end-of-central-directory record (PK\\x05\\x06) with the right entry count", () => {
    const eocd = zip.slice(zip.length - 22);
    expect([...eocd.slice(0, 4)]).toEqual([0x50, 0x4b, 0x05, 0x06]);
    expect(eocd[10] | (eocd[11] << 8)).toBe(2);
  });

  it("embeds both file paths + their contents verbatim", () => {
    const s = new TextDecoder().decode(zip);
    expect(s).toContain("proj/scene.json");
    expect(s).toContain('{"name":"x"}');
    expect(s).toContain("proj/src/index.ts");
    expect(s).toContain("export function main() {}");
  });

  it("has a central-directory header (PK\\x01\\x02) per entry", () => {
    let count = 0;
    for (let i = 0; i + 4 <= zip.length; i++) {
      if (zip[i] === 0x50 && zip[i + 1] === 0x4b && zip[i + 2] === 0x01 && zip[i + 3] === 0x02) count++;
    }
    expect(count).toBe(2);
  });
});
