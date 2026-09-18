import { describe, expect, it } from "vitest";

import { governanceApiBase } from "./api-base";

describe("governanceApiBase", () => {
  it("defaults to this node, never to the upstream Decentraland DAO, and an explicit override wins over both env names", () => {
    const base = governanceApiBase(undefined, {});
    expect(base).toMatch(/\/governance-api$/);
    expect(base).not.toContain("decentraland.org");

    expect(
      governanceApiBase("http://override.test/gov/", {
        GOVERNANCE_READ_URL: "http://127.0.0.1:5151",
      }),
    ).toBe("http://override.test/gov");
  });

  it("prefers GOVERNANCE_READ_URL over the legacy GOVERNANCE_API_URL, which is still honoured with its trailing slash trimmed", () => {
    expect(
      governanceApiBase(undefined, {
        GOVERNANCE_READ_URL: "http://127.0.0.1:5151",
        GOVERNANCE_API_URL: "https://legacy.example/api",
      }),
    ).toBe("http://127.0.0.1:5151");
    expect(
      governanceApiBase(undefined, { GOVERNANCE_API_URL: "http://127.0.0.1:5151/" }),
    ).toBe("http://127.0.0.1:5151");
  });
});
