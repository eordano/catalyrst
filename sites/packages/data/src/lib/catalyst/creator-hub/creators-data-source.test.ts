import { describe, expect, it } from "vitest";

import { classifyMetricsArtifact, worldMetricsPath } from "./creators-data-source";
import { loadWorldMetricsArtifact } from "./creators-data-source.server";

const BASE = "https://creators-data.example.test/api";

describe("classifyMetricsArtifact", () => {
  it("maps source: fixture to unavailable, naming the export date", () => {
    const v = classifyMetricsArtifact({
      source: "fixture",
      exportedAt: "2026-07-10",
      metrics: { visits: 409 },
    });
    expect(v.kind).toBe("unavailable");
    if (v.kind !== "unavailable") throw new Error("unreachable");
    expect(v.reason).toContain("source: fixture");
    expect(v.reason).toContain("2026-07-10");
    expect(v.reason).toContain("No values are shown");
  });

  it("maps only source: metabase to snapshot", () => {
    const v = classifyMetricsArtifact({
      source: "metabase",
      exportedAt: "2026-07-10",
      metrics: { visits: 409 },
    });
    expect(v.kind).toBe("snapshot");
    if (v.kind !== "snapshot") throw new Error("unreachable");
    expect(v.exportSource).toBe("metabase");
    expect(v.value).toEqual({ visits: 409 });
  });

  it("refuses an artifact that does not say where it came from", () => {
    expect(classifyMetricsArtifact({ metrics: {} }).kind).toBe("unavailable");
  });

  it("refuses a non-object payload (the host serves HTML today)", () => {
    expect(classifyMetricsArtifact("<!doctype html>").kind).toBe("unavailable");
  });
});

describe("loadWorldMetricsArtifact", () => {
  it("builds the documented path", () => {
    expect(worldMetricsPath("041.dcl.eth")).toBe("/worlds/041.dcl.eth/metrics");
  });

  it("a fixture artifact never becomes a snapshot", async () => {
    const d = await loadWorldMetricsArtifact("041.dcl.eth", {
      base: BASE,
      fetchImpl: (async () =>
        new Response(
          JSON.stringify({ source: "fixture", exportedAt: "2026-07-10", metrics: {} }),
          { status: 200 },
        )) as unknown as typeof fetch,
    });
    expect(d.state).toBe("unavailable");
    expect(Object.keys(d)).not.toContain("value");
  });

  it("a metabase artifact becomes a snapshot", async () => {
    const d = await loadWorldMetricsArtifact("041.dcl.eth", {
      base: BASE,
      fetchImpl: (async () =>
        new Response(
          JSON.stringify({
            source: "metabase",
            exportedAt: "2026-07-10",
            metrics: { visits: 1 },
          }),
          { status: 200 },
        )) as unknown as typeof fetch,
    });
    expect(d.state).toBe("snapshot");
    if (d.state !== "snapshot") throw new Error("unreachable");
    expect(d.exportSource).toBe("metabase");
  });

  it("an HTML response (the live behaviour today) is unavailable", async () => {
    const d = await loadWorldMetricsArtifact("041.dcl.eth", {
      base: BASE,
      fetchImpl: (async () =>
        new Response("<!doctype html><html></html>", {
          status: 200,
        })) as unknown as typeof fetch,
    });
    expect(d.state).toBe("unavailable");
    if (d.state !== "unavailable") throw new Error("unreachable");
    expect(d.reason).toContain("creators-data.example.test");
  });
});
