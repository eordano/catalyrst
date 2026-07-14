import assert from "node:assert";
import fs from "node:fs";
import path from "node:path";
import { describe, it } from "node:test";
import { deploymentSchema, requireDeploymentSchema } from "./deployment-schema";

const PACKAGE_ROOT = path.resolve(__dirname, "..", "..", "..");

const read = (relative: string): string =>
  fs.readFileSync(path.join(PACKAGE_ROOT, relative), "utf-8");

const withEnv = (
  env: { SQUID_SCHEMA?: string; DB_SCHEMA?: string },
  fn: () => void
): void => {
  const previous = {
    SQUID_SCHEMA: process.env.SQUID_SCHEMA,
    DB_SCHEMA: process.env.DB_SCHEMA,
  };
  for (const key of ["SQUID_SCHEMA", "DB_SCHEMA"] as const) {
    if (env[key] === undefined) delete process.env[key];
    else process.env[key] = env[key];
  }
  try {
    fn();
  } finally {
    for (const key of ["SQUID_SCHEMA", "DB_SCHEMA"] as const) {
      if (previous[key] === undefined) delete process.env[key];
      else process.env[key] = previous[key];
    }
  }
};

describe("deployment schema env var", () => {
  it("indexer.sh exports SQUID_SCHEMA and unsets DB_SCHEMA", () => {
    const script = read("indexer.sh");
    assert.match(script, /^export SQUID_SCHEMA=\$NEW_SCHEMA_NAME$/m);
    assert.match(script, /^unset DB_SCHEMA$/m);
    assert.doesNotMatch(script, /^export DB_SCHEMA/m);
  });

  it("restart.sh exports SQUID_SCHEMA and unsets DB_SCHEMA", () => {
    const script = read("restart.sh");
    assert.match(script, /^export SQUID_SCHEMA$/m);
    assert.match(script, /^unset DB_SCHEMA$/m);
    assert.doesNotMatch(script, /^export DB_SCHEMA/m);
  });

  // Our units exec the nix wrappers, never indexer.sh/restart.sh, so no source may
  // read process.env directly: every one of them must go through the helpers, which
  // still accept the not-yet-renamed DB_SCHEMA the deployed environment carries.
  // The processor entry points take the strict helper -- an unresolved schema there
  // silently becomes stateSchema `<chain>_processor_undefined` and re-indexes from
  // genesis over the live data, so it must abort the process instead.
  for (const source of ["src/eth/main.ts", "src/polygon/main.ts"]) {
    it(`${source} resolves the schema through requireDeploymentSchema()`, () => {
      const contents = read(source);
      assert.match(contents, /const schemaName = requireDeploymentSchema\(\);/);
      assert.doesNotMatch(contents, /const schemaName = deploymentSchema\(\);/);
      assert.doesNotMatch(contents, /process\.env\.SQUID_SCHEMA/);
      assert.doesNotMatch(contents, /process\.env\.DB_SCHEMA/);
    });
  }

  // head-notification.ts stays on the permissive helper on purpose: it resolves at
  // module-import time, before either entry point's guard could run, and already
  // degrades to the unqualified `head_sync_status` table in local dev. Do not
  // promote it to the strict helper.
  it("head-notification.ts keeps the permissive deploymentSchema()", () => {
    const contents = read("src/common/utils/head-notification.ts");
    assert.match(contents, /deploymentSchema\(\)/);
    assert.doesNotMatch(contents, /requireDeploymentSchema/);
    assert.doesNotMatch(contents, /process\.env\.SQUID_SCHEMA/);
    assert.doesNotMatch(contents, /process\.env\.DB_SCHEMA/);
  });
});

describe("deploymentSchema()", () => {
  it("prefers SQUID_SCHEMA when both are set", () => {
    withEnv({ SQUID_SCHEMA: "marketplace_squid_123", DB_SCHEMA: "legacy" }, () => {
      assert.strictEqual(deploymentSchema(), "marketplace_squid_123");
    });
  });

  it("uses SQUID_SCHEMA when it is the only one set", () => {
    withEnv({ SQUID_SCHEMA: "marketplace_squid_123" }, () => {
      assert.strictEqual(deploymentSchema(), "marketplace_squid_123");
    });
  });

  it("falls back to DB_SCHEMA when SQUID_SCHEMA is unset", () => {
    withEnv({ DB_SCHEMA: "squid_marketplace" }, () => {
      assert.strictEqual(deploymentSchema(), "squid_marketplace");
    });
  });

  it("falls back to DB_SCHEMA when SQUID_SCHEMA is empty", () => {
    withEnv({ SQUID_SCHEMA: "", DB_SCHEMA: "squid_marketplace" }, () => {
      assert.strictEqual(deploymentSchema(), "squid_marketplace");
    });
  });

  it("returns undefined when neither is set", () => {
    withEnv({}, () => {
      assert.strictEqual(deploymentSchema(), undefined);
    });
  });

  it("returns undefined when both are set to the empty string", () => {
    withEnv({ SQUID_SCHEMA: "", DB_SCHEMA: "" }, () => {
      assert.strictEqual(deploymentSchema(), undefined);
    });
  });

  it("returns undefined when DB_SCHEMA is whitespace only", () => {
    withEnv({ DB_SCHEMA: "   " }, () => {
      assert.strictEqual(deploymentSchema(), undefined);
    });
  });
});

describe("requireDeploymentSchema()", () => {
  it("prefers SQUID_SCHEMA when both are set", () => {
    withEnv({ SQUID_SCHEMA: "marketplace_squid_123", DB_SCHEMA: "legacy" }, () => {
      assert.strictEqual(requireDeploymentSchema(), "marketplace_squid_123");
    });
  });

  it("uses SQUID_SCHEMA when it is the only one set", () => {
    withEnv({ SQUID_SCHEMA: "marketplace_squid_123" }, () => {
      assert.strictEqual(requireDeploymentSchema(), "marketplace_squid_123");
    });
  });

  it("accepts the deprecated DB_SCHEMA when SQUID_SCHEMA is unset", () => {
    withEnv({ DB_SCHEMA: "squid_marketplace" }, () => {
      assert.strictEqual(requireDeploymentSchema(), "squid_marketplace");
    });
  });

  it("throws naming SQUID_SCHEMA when neither is set", () => {
    withEnv({}, () => {
      assert.throws(() => requireDeploymentSchema(), /SQUID_SCHEMA is not set/);
      assert.throws(() => requireDeploymentSchema(), /DB_SCHEMA/);
    });
  });

  it("throws when both are set but empty", () => {
    withEnv({ SQUID_SCHEMA: "", DB_SCHEMA: "" }, () => {
      assert.throws(() => requireDeploymentSchema(), /SQUID_SCHEMA is not set/);
    });
  });

  it("throws when SQUID_SCHEMA is whitespace only", () => {
    withEnv({ SQUID_SCHEMA: "  " }, () => {
      assert.throws(() => requireDeploymentSchema(), /SQUID_SCHEMA is not set/);
    });
  });

  it("trims the surrounding whitespace an EnvironmentFile can leave", () => {
    withEnv({ SQUID_SCHEMA: " squid_marketplace " }, () => {
      assert.strictEqual(requireDeploymentSchema(), "squid_marketplace");
    });
  });

  it("rejects a name that is not a bare SQL identifier", () => {
    withEnv({ SQUID_SCHEMA: "bad-name" }, () => {
      assert.throws(() => requireDeploymentSchema(), /bare SQL identifier/);
    });
  });

  it("rejects a quoted name that would break the head_sync_status identifier", () => {
    withEnv({ SQUID_SCHEMA: 'squid_marketplace"' }, () => {
      assert.throws(() => requireDeploymentSchema(), /bare SQL identifier/);
    });
  });
});
