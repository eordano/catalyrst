import { createServer, type Server } from "node:http";

import { afterAll, beforeAll, beforeEach, describe, expect, it } from "vitest";

import { clearProbeSnapshot, probeService, probeSnapshot } from "./probe.server";
import type { OperatorService } from "./registry";

let server: Server;
let port: number;
let healthStatus = 200;

beforeAll(async () => {
  server = createServer((req, res) => {
    if (req.url === "/health") {
      res.writeHead(healthStatus);
      res.end("ok");
    } else {
      res.writeHead(500);
      res.end("boom");
    }
  });
  await new Promise<void>((resolve) => server.listen(0, "127.0.0.1", resolve));
  const addr = server.address();
  port = typeof addr === "object" && addr ? addr.port : 0;
});

beforeEach(() => {
  healthStatus = 200;
  clearProbeSnapshot();
});

afterAll(async () => {
  await new Promise((resolve) => server.close(resolve));
});

function svc(overrides: Partial<OperatorService>): OperatorService {
  return {
    key: "t",
    name: "t",
    unit: "t",
    port,
    healthPath: "/health",
    expect: "2xx",
    serves: "t",
    ...overrides,
  };
}

describe("probeService", () => {
  it("classifies a 2xx health answer as ok", async () => {
    const r = await probeService(svc({}));
    expect(r.state).toBe("ok");
    expect(r.httpStatus).toBe(200);
  });

  it("classifies a non-2xx answer as answering when 2xx is expected", async () => {
    const r = await probeService(svc({ healthPath: "/nope" }));
    expect(r.state).toBe("answering");
    expect(r.httpStatus).toBe(500);
    expect(r.detail).toContain("HTTP 500");
  });

  it("classifies any HTTP answer as ok under any-http", async () => {
    const r = await probeService(svc({ healthPath: "/nope", expect: "any-http" }));
    expect(r.state).toBe("ok");
  });

  it("classifies a closed port as down with a named reason", async () => {
    const r = await probeService(svc({ port: 1 }));
    expect(r.state).toBe("down");
    expect(r.httpStatus).toBeNull();
    expect(r.detail.length).toBeGreaterThan(0);
  });
});

describe("probeSnapshot", () => {
  it("scoped recheck re-probes only the named services and keeps the rest cached", async () => {
    const a = svc({ key: "a" });
    const b = svc({ key: "b", port: 1 });
    const first = await probeSnapshot([a, b]);
    expect(first.map((p) => [p.key, p.state])).toEqual([
      ["a", "ok"],
      ["b", "down"],
    ]);

    healthStatus = 500;
    const scoped = await probeSnapshot([a, b], { only: ["b"] });
    const scopedA = scoped.find((p) => p.key === "a");
    const scopedB = scoped.find((p) => p.key === "b");
    expect(scopedA?.state).toBe("ok");
    expect(scopedA?.probedAt).toBe(first[0].probedAt);
    expect(scopedB?.state).toBe("down");
    expect(scopedB?.probedAt).toBeGreaterThanOrEqual(first[1].probedAt);

    const recheckedA = await probeSnapshot([a, b], { only: ["a"] });
    expect(recheckedA.find((p) => p.key === "a")?.state).toBe("answering");
  });

  it("probes never-seen services even when the scope excludes them", async () => {
    const a = svc({ key: "a" });
    const b = svc({ key: "b", port: 1 });
    await probeSnapshot([a], {});
    const withNew = await probeSnapshot([a, b], { only: ["a"] });
    expect(withNew.find((p) => p.key === "b")?.state).toBe("down");
  });
});
