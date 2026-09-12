import { assertNotNull } from "@subsquid/util-internal";

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
