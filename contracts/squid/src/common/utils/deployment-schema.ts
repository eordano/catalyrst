import { assertNotNull } from "@subsquid/util-internal";

// Resolves the deployment's schema identity. Upstream reads SQUID_SCHEMA alone
// because indexer.sh / restart.sh always export it before starting a processor. Our
// units exec the nix wrappers (catalyrst/nix/squid.nix) straight onto lib/*/main.js
// with an EnvironmentFile, so those scripts never run and the deployed environment
// still carries the old name. Keep DB_SCHEMA as a DEPRECATED read-only fallback until
// the deployment's squid env is renamed: without it a rebuild would start both processors
// with stateSchema `<chain>_processor_undefined`, losing the sync cursor and
// re-indexing from genesis over the live data. Drop the fallback once the deployed
// environment sets SQUID_SCHEMA.
//
// Reading DB_SCHEMA does NOT restore the search_path pin upstream removed in #118:
// @subsquid/typeorm-config only emits `options=-c search_path="<name>"` when
// DB_SCHEMA is SET in the process environment, which is orthogonal to reading it here.
let fallbackWarned = false;

export const deploymentSchema = (): string | undefined => {
  const schema = process.env.SQUID_SCHEMA?.trim();
  if (schema) return schema;
  const legacy = process.env.DB_SCHEMA?.trim();
  if (!legacy) return undefined;
  if (!fallbackWarned) {
    fallbackWarned = true;
    console.log(
      "[SCHEMA] SQUID_SCHEMA unset; falling back to deprecated DB_SCHEMA"
    );
  }
  return legacy;
};

const BARE_IDENTIFIER = /^[A-Za-z_][A-Za-z0-9_$]*$/;

export const requireDeploymentSchema = (): string => {
  const name = assertNotNull(
    deploymentSchema(),
    "SQUID_SCHEMA is not set (DB_SCHEMA is accepted as a deprecated fallback). " +
      "Refusing to start: an unresolved schema yields stateSchema " +
      "`<chain>_processor_undefined`, which re-indexes from genesis over the live data."
  );
  if (!BARE_IDENTIFIER.test(name)) {
    throw new Error(
      `Deployment schema ${JSON.stringify(name)} is not a bare SQL identifier; ` +
        "refusing to start (SQUID_SCHEMA / deprecated DB_SCHEMA)."
    );
  }
  return name;
};
