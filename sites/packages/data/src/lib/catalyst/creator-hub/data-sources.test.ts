import { describe, expect, it } from "vitest";

import { SOURCE_REGISTRY, isProbeable, sourcesByClass } from "./data-sources";
import { probeSources, sourceRegistry } from "./data-sources.server";

describe("the ledger invariant", () => {
  it("a probe is defined if and only if the row claims live or sampled", () => {
    for (const entry of sourceRegistry()) {
      expect(
        Boolean(entry.probe),
        `${entry.id} (${entry.klass}) probe presence`,
      ).toBe(isProbeable(entry.klass));
    }
  });

  it("no entry carries both a probe and a literal value", () => {
    for (const entry of sourceRegistry()) {
      expect(entry).not.toHaveProperty("value");
    }
  });

  it("every row names an endpoint, a note and a unique id", () => {
    const ids = new Set<string>();
    for (const entry of SOURCE_REGISTRY) {
      expect(entry.id, "id is unique").not.toBe("");
      expect(ids.has(entry.id), `${entry.id} is unique`).toBe(false);
      ids.add(entry.id);
      expect(entry.note.length, `${entry.id} has a reason`).toBeGreaterThan(20);
      expect(entry.endpoint, `${entry.id} names an endpoint or "\u{2014}"`).toBeTruthy();
    }
  });

  it("only unbuilt rows carry a 'today' escape hatch", () => {
    for (const entry of SOURCE_REGISTRY) {
      if (entry.today) expect(entry.klass).toBe("unbuilt");
    }
  });

  it("the snapshot group is empty and says so", () => {
    const groups = sourcesByClass();
    const snapshot = groups.find((g) => g.klass === "snapshot");
    expect(snapshot?.entries).toEqual([]);
    expect(snapshot?.note).toContain("nothing currently qualifies");
  });

  it("carries the known-bad numbers as excluded rows so nobody rewires them", () => {
    const excluded = SOURCE_REGISTRY.filter((e) => e.klass === "excluded").map(
      (e) => e.id,
    );
    expect(excluded).toContain("hot-scenes");
    expect(excluded).toContain("places-user-visits");
    expect(excluded).toContain("occupancy-totals");
    expect(excluded).toContain("worlds-base");
  });

  it("uses no banned placeholder copy", () => {
    const banned = /\b(n\/a|coming soon|under construction|TBD)\b/i;
    for (const entry of SOURCE_REGISTRY) {
      expect(banned.test(entry.note), `${entry.id} note`).toBe(false);
      expect(banned.test(entry.datum), `${entry.id} datum`).toBe(false);
    }
  });
});

describe("probeSources", () => {
  const down = (async () => {
    throw new Error("everything is down");
  }) as unknown as typeof fetch;

  it("probes only the probeable rows and leaves the constants alone", async () => {
    const rows = await probeSources({ fetchImpl: down });
    for (const row of rows) {
      if (isProbeable(row.klass)) expect(row.result, row.id).toBeDefined();
      else expect(row.result, row.id).toBeUndefined();
    }
  });

  it("cannot claim live for something that is down", async () => {
    const rows = await probeSources({ fetchImpl: down, address: "0xabc" });
    const probed = rows.filter((r) => r.result);
    expect(probed.length).toBeGreaterThan(0);
    for (const row of probed) {
      expect(row.result?.state, row.id).not.toBe("live");
      expect(row.result?.state, row.id).not.toBe("sampled");
    }
  });

  it("says 'not probed' rather than 'unavailable' when it was given no subject", async () => {
    const rows = await probeSources({ fetchImpl: down });
    const worldRow = rows.find((r) => r.id === "world-about");
    expect(worldRow?.result?.state).toBe("no-sample");
  });
});
