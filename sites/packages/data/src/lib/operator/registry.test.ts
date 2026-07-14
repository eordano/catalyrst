import { describe, expect, it } from "vitest";

import { KNOWN_ENV, SERVICES, isSecretName } from "./registry";

describe("operator service registry", () => {
  it("has unique keys, units and ports", () => {
    const keys = SERVICES.map((s) => s.key);
    const units = SERVICES.map((s) => s.unit);
    const ports = SERVICES.map((s) => s.port);
    expect(new Set(keys).size).toBe(keys.length);
    expect(new Set(units).size).toBe(units.length);
    expect(new Set(ports).size).toBe(ports.length);
  });

  it("probes rooted paths on registered local ports", () => {
    for (const s of SERVICES) {
      expect(s.healthPath.startsWith("/")).toBe(true);
      expect(s.port).toBeGreaterThan(1024);
      expect(s.serves.length).toBeGreaterThan(0);
    }
  });

  it("catalogs every env var exactly once", () => {
    const names = KNOWN_ENV.map((v) => v.name);
    expect(new Set(names).size).toBe(names.length);
  });
});

describe("isSecretName", () => {
  it("masks credential-shaped names and leaves plain ones alone", () => {
    expect(isSecretName("CATALYST_DATABASE_URL")).toBe(true);
    expect(isSecretName("SOME_API_TOKEN")).toBe(true);
    expect(isSecretName("LIVEKIT_API_SECRET")).toBe(true);
    expect(isSecretName("OPERATOR_PROBE_HOST")).toBe(false);
    expect(isSecretName("ADMIN_WALLETS")).toBe(false);
  });
});
