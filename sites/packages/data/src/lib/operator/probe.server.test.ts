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
  it("classifies 2xx as ok, non-2xx as answering under 2xx but ok under any-http, and a closed port as down with a named reason", async () => {
    const ok = await probeService(svc({}));
    expect(ok.state).toBe("ok");
    expect(ok.httpStatus).toBe(200);

    const answering = await probeService(svc({ healthPath: "/nope" }));
    expect(answering.state).toBe("answering");
    expect(answering.httpStatus).toBe(500);
    expect(answering.detail).toContain("HTTP 500");

    const anyHttp = await probeService(svc({ healthPath: "/nope", expect: "any-http" }));
    expect(anyHttp.state).toBe("ok");

    const down = await probeService(svc({ port: 1 }));
    expect(down.state).toBe("down");
    expect(down.httpStatus).toBeNull();
    expect(down.detail.length).toBeGreaterThan(0);
  });
});

describe("probeSnapshot", () => {
  it("a scoped recheck re-probes only the named services, keeps the rest cached, and still probes never-seen services outside the scope", async () => {
    const a = svc({ key: "a" });
    const b = svc({ key: "b", port: 1 });
    const c = svc({ key: "c", port: 1 });
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

    const withNew = await probeSnapshot([a, b, c], { only: ["b"] });
    expect(withNew.find((p) => p.key === "c")?.state).toBe("down");
    expect(withNew.find((p) => p.key === "a")?.state).toBe("answering");
  });
});
