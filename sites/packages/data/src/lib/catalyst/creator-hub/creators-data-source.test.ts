import { describe, expect, it } from "vitest";

import { classifyMetricsArtifact, worldMetricsPath } from "./creators-data-source";
import { loadWorldMetricsArtifact } from "./creators-data-source.server";

const BASE = "https://creators-data.example.test/api";

function artifactFetch(body: string): typeof fetch {
  return (async () => new Response(body, { status: 200 })) as unknown as typeof fetch;
}

describe("classifyMetricsArtifact", () => {
  it("only source: metabase becomes a snapshot; fixture, unsourced and non-object payloads are unavailable", () => {
    const fixture = classifyMetricsArtifact({
      source: "fixture",
      exportedAt: "2026-07-10",
      metrics: { visits: 409 },
    });
    expect(fixture.kind).toBe("unavailable");
    if (fixture.kind !== "unavailable") throw new Error("unreachable");
    expect(fixture.reason).toContain("source: fixture");
    expect(fixture.reason).toContain("2026-07-10");
    expect(fixture.reason).toContain("No values are shown");

    const snapshot = classifyMetricsArtifact({
      source: "metabase",
      exportedAt: "2026-07-10",
      metrics: { visits: 409 },
    });
    expect(snapshot.kind).toBe("snapshot");
    if (snapshot.kind !== "snapshot") throw new Error("unreachable");
    expect(snapshot.exportSource).toBe("metabase");
    expect(snapshot.value).toEqual({ visits: 409 });

    expect(classifyMetricsArtifact({ metrics: {} }).kind).toBe("unavailable");
    expect(classifyMetricsArtifact("<!doctype html>").kind).toBe("unavailable");
  });
});

describe("loadWorldMetricsArtifact", () => {
  it("reads the documented path; a fixture artifact never becomes a snapshot, a metabase one does", async () => {
    expect(worldMetricsPath("041.dcl.eth")).toBe("/worlds/041.dcl.eth/metrics");
    const fixture = await loadWorldMetricsArtifact("041.dcl.eth", {
      base: BASE,
      fetchImpl: artifactFetch(
        JSON.stringify({ source: "fixture", exportedAt: "2026-07-10", metrics: {} }),
      ),
    });
    expect(fixture.state).toBe("unavailable");
    expect(Object.keys(fixture)).not.toContain("value");

    const snapshot = await loadWorldMetricsArtifact("041.dcl.eth", {
      base: BASE,
      fetchImpl: artifactFetch(
        JSON.stringify({ source: "metabase", exportedAt: "2026-07-10", metrics: { visits: 1 } }),
      ),
    });
    expect(snapshot.state).toBe("snapshot");
    if (snapshot.state !== "snapshot") throw new Error("unreachable");
    expect(snapshot.exportSource).toBe("metabase");
  });

  it("an HTML response (the live behaviour today) is unavailable and names the host", async () => {
    const d = await loadWorldMetricsArtifact("041.dcl.eth", {
      base: BASE,
      fetchImpl: artifactFetch("<!doctype html><html></html>"),
    });
    expect(d.state).toBe("unavailable");
    if (d.state !== "unavailable") throw new Error("unreachable");
    expect(d.reason).toContain("creators-data.example.test");
  });
});
