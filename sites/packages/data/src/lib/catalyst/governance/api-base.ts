import { catalystBase } from "../client";

/**
 * Single source of truth for the governance read base URL.
 *
 * Before this module the helper was copy-pasted into 16 files with two
 * divergent defaults: half fell back to `${catalystBase()}/governance` (this
 * node's own catalyrst-governance) and half to
 * `https://governance.decentraland.org/api` (the upstream Decentraland DAO).
 * The upstream default is wrong for a realm: it silently serves another
 * deployment's data, and it is what let `/budget/all` (an upstream-only path)
 * look plausible for the lifetime of grant-budget.ts. The only default is now
 * this node.
 *
 * Env: `GOVERNANCE_READ_URL` is the correct name — it is the governance
 * service this app *reads from*. `GOVERNANCE_API_URL` is accepted as a legacy
 * alias because it is set on the deployed host today, but it is overloaded:
 * catalyrst-governance itself uses `GOVERNANCE_API_URL` to mean the upstream
 * DAO API it *syncs from* (catalyrst-governance/src/config.rs:130). Neither
 * name is checked into deploy/, nixos/ or rig/ — see the governance
 * report; the working deployment is not reproducible from the repo.
 */
export type GovernanceEnv = Record<string, string | undefined>;

export function governanceProcessEnv(): GovernanceEnv {
  return typeof process !== "undefined" && process.env ? process.env : {};
}

export function governanceApiBase(
  override?: string,
  env: GovernanceEnv = governanceProcessEnv(),
): string {
  const base =
    override ??
    env.GOVERNANCE_READ_URL ??
    env.GOVERNANCE_API_URL ??
    `${catalystBase()}/governance`;
  return base.replace(/\/$/, "");
}
