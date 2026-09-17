import assert from "node:assert";
import { describe, it } from "node:test";

import { DataType, buildData } from "./data";
import { stripNul } from "./utils";

describe("stripNul", () => {
  it("removes an embedded NUL byte that PostgreSQL TEXT would reject", () => {
    assert.strictEqual(stripNul("0,Estate\u0000Name,desc"), "0,EstateName,desc");
  });

  it("removes several NUL bytes wherever they appear", () => {
    assert.strictEqual(stripNul("\u0000a\u0000b\u0000"), "ab");
  });

  it("leaves a string without NUL bytes unchanged", () => {
    const value = "0,My Estate,A nice place,ipns://hash";
    assert.strictEqual(stripNul(value), value);
  });

  it("returns an empty string unchanged", () => {
    assert.strictEqual(stripNul(""), "");
  });

  it("keeps other control and multibyte characters", () => {
    assert.strictEqual(
      stripNul("tab\tnew\nline \u00f1 \u4e16"),
      "tab\tnew\nline \u00f1 \u4e16"
    );
  });
});

describe("buildData", () => {
  it("strips NUL bytes out of every TEXT field it fills", () => {
    const entity = buildData(
      "asset-1",
      '0,"My\u0000Estate","A nice\u0000place","ipns://ha\u0000sh"',
      DataType.ESTATE
    );
    assert.ok(entity);
    assert.strictEqual(entity.name, "MyEstate");
    assert.strictEqual(entity.description, "A niceplace");
    assert.strictEqual(entity.ipns, "ipns://hash");
  });

  it("accepts a payload whose version marker is preceded by a NUL byte", () => {
    const entity = buildData(
      "asset-2",
      '\u00000,"My Parcel","A nice place"',
      DataType.PARCEL
    );
    assert.ok(entity);
    assert.strictEqual(entity.version, "0");
    assert.strictEqual(entity.name, "My Parcel");
  });

  it("still rejects a payload that is invalid once the NUL bytes are gone", () => {
    assert.strictEqual(
      buildData("asset-3", '\u00001,"name","desc"', DataType.PARCEL),
      null
    );
  });
});
