import { catalystBase } from "../client";

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
    `${catalystBase()}/governance-api`;
  return base.replace(/\/$/, "");
}
