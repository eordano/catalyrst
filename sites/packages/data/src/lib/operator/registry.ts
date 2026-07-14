/**
 * The operator-facing probe map for the /server page. Service topology --
 * keys, units, ports, health endpoints, bundle membership -- is generated
 * from catalyrst/nixos/facts.nix (the canonical source) into
 * services.generated.ts by scripts/gen-operator-registry.mts; the drift gate
 * is `npm run gen:operator:check`. A bundle carrier that hosts several facts
 * services lists them in `members` and counts as enabled when any member is.
 */

export type HealthExpectation = "2xx" | "any-http";

export type OperatorService = {
  /** facts.nix service key, or a carrier alias when `members` is set. */
  key: string;
  name: string;
  /** systemd unit name in the exported nixos module (actionable commands). */
  unit: string;
  port: number;
  healthPath: string;
  /** "any-http": any HTTP answer counts as up (no known health endpoint). */
  expect: HealthExpectation;
  /** One line: what stops working when this service is down. */
  serves: string;
  /** facts.nix service keys this carrier hosts (bundle units only). */
  members?: string[];
};

export { SERVICES } from "./services.generated";

export type KnownEnvVar = {
  name: string;
  purpose: string;
  /** Service keys (or "sites") that read this variable. */
  consumers: string[];
  example?: string;
};

export const KNOWN_ENV: KnownEnvVar[] = [
  {
    name: "ADMIN_WALLETS",
    purpose:
      "Comma-separated wallet allowlist for /admin and /server. Unset, /server trusts the edge allowlist alone.",
    consumers: ["sites"],
    example: "0xabc...,0xdef...",
  },
  {
    name: "CATALYST_URL",
    purpose: "Base URL SSR uses for catalyst reads. Unset, same-origin is assumed.",
    consumers: ["sites"],
    example: "http://127.0.0.1:5141",
  },
  {
    name: "WORLDS_URL",
    purpose: "Worlds content server base URL. Unset, derived as worlds.<domain>.",
    consumers: ["sites"],
  },
  {
    name: "CATALYST_DATABASE_URL",
    purpose: "Postgres DSN for direct place/category reads.",
    consumers: ["sites"],
    example: "postgresql:///content?host=/run/postgresql&user=catalyrst",
  },
  {
    name: "SYSTEM_STATUS_FILE",
    purpose: "Snapshot JSON the /admin systems panel reads, produced by a collector timer.",
    consumers: ["sites"],
  },
  {
    name: "CATALYRST_ENABLED_SERVICES",
    purpose:
      "facts.nix service keys this node runs, set by the nixos module. Unset, /server treats every service as enabled.",
    consumers: ["sites"],
  },
  {
    name: "CATALYRST_OPERATOR_ENV_FILE",
    purpose:
      "Where this page persists operator env vars. The sites unit also loads it at start, so saved values apply after a restart.",
    consumers: ["sites"],
    example: "/var/lib/catalyrst-sites/operator.env",
  },
  {
    name: "OPERATOR_PROBE_HOST",
    purpose: "Host the /server health probes dial. Unset, 127.0.0.1.",
    consumers: ["sites"],
  },
];

export function knownEnv(name: string): KnownEnvVar | undefined {
  return KNOWN_ENV.find((v) => v.name === name);
}

const SECRET_NAME_RE = /(TOKEN|SECRET|KEY|PASSWORD|DSN|DATABASE_URL|CONNECTION_STRING)/;

/** Values whose names match are masked in the UI and never echoed back. */
export function isSecretName(name: string): boolean {
  return SECRET_NAME_RE.test(name);
}
