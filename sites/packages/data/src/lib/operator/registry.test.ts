import { describe, expect, it } from "vitest";

import { KNOWN_ENV, SERVICES, isSecretName } from "./registry";

describe("operator service registry", () => {
  it("has unique keys, units and ports, probes rooted paths on registered local ports, and catalogs every env var exactly once", () => {
    const keys = SERVICES.map((s) => s.key);
    const units = SERVICES.map((s) => s.unit);
    const ports = SERVICES.map((s) => s.port);
    expect(new Set(keys).size).toBe(keys.length);
    expect(new Set(units).size).toBe(units.length);
    expect(new Set(ports).size).toBe(ports.length);

    const offenders = SERVICES.filter(
      (s) => !s.healthPath.startsWith("/") || s.port <= 1024 || s.serves.length === 0,
    ).map((s) => s.key);
    expect(offenders).toEqual([]);

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
