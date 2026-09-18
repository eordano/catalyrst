import { describe, expect, it } from "vitest";

import { SOURCE_REGISTRY, isProbeable, sourcesByClass } from "./data-sources";
import { probeSources, sourceRegistry } from "./data-sources.server";

describe("the ledger invariant", () => {
  it("a probe is defined iff the row claims live or sampled, and no entry carries a literal value", () => {
    for (const entry of sourceRegistry()) {
      expect(
        Boolean(entry.probe),
        `${entry.id} (${entry.klass}) probe presence`,
      ).toBe(isProbeable(entry.klass));
      expect(entry).not.toHaveProperty("value");
    }
  });

  it("every row names an endpoint, a reason and a unique id, only unbuilt rows carry a today escape hatch, and no placeholder copy appears", () => {
    const ids = new Set<string>();
    const banned = /\b(n\/a|coming soon|under construction|TBD)\b/i;
    for (const entry of SOURCE_REGISTRY) {
      expect(entry.id, "id is unique").not.toBe("");
      expect(ids.has(entry.id), `${entry.id} is unique`).toBe(false);
      ids.add(entry.id);
      expect(entry.note.length, `${entry.id} has a reason`).toBeGreaterThan(20);
      expect(entry.endpoint, `${entry.id} names an endpoint`).toBeTruthy();
      if (entry.today) expect(entry.klass).toBe("unbuilt");
      expect(banned.test(entry.note), `${entry.id} note`).toBe(false);
      expect(banned.test(entry.datum), `${entry.id} datum`).toBe(false);
    }
  });

  it("the snapshot group is empty and says so, and the known-bad numbers stay excluded", () => {
    const snapshot = sourcesByClass().find((g) => g.klass === "snapshot");
    expect(snapshot?.entries).toEqual([]);
    expect(snapshot?.note).toContain("nothing currently qualifies");
    const excluded = SOURCE_REGISTRY.filter((e) => e.klass === "excluded").map((e) => e.id);
    expect(excluded).toContain("hot-scenes");
    expect(excluded).toContain("places-user-visits");
    expect(excluded).toContain("occupancy-totals");
    expect(excluded).toContain("worlds-base");
  });
});

describe("probeSources", () => {
  const down = (async () => {
    throw new Error("everything is down");
  }) as unknown as typeof fetch;

  it("probes only the probeable rows and cannot claim live or sampled for something that is down", async () => {
    const rows = await probeSources({ fetchImpl: down, address: "0xabc" });
    const probed = rows.filter((r) => r.result);
    expect(probed.length).toBeGreaterThan(0);
    for (const row of rows) {
      if (isProbeable(row.klass)) expect(row.result, row.id).toBeDefined();
      else expect(row.result, row.id).toBeUndefined();
      if (row.result) {
        expect(row.result.state, row.id).not.toBe("live");
        expect(row.result.state, row.id).not.toBe("sampled");
      }
    }
  });

  it("says 'not probed' rather than 'unavailable' when it was given no subject", async () => {
    const rows = await probeSources({ fetchImpl: down });
    const worldRow = rows.find((r) => r.id === "world-about");
    expect(worldRow?.result?.state).toBe("no-sample");
  });
});
