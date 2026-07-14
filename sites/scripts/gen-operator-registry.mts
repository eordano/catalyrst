import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const SITES = fileURLToPath(new URL("..", import.meta.url));
const FACTS = join(SITES, "..", "nixos", "facts.nix");
const OUT = join(SITES, "packages", "data", "src", "lib", "operator", "services.generated.ts");

function fail(msg: string): never {
  console.error(`gen-operator-registry: ${msg}`);
  process.exit(1);
}

type FactsService = {
  unit: string;
  port: number;
  healthPath: string | null;
  healthExpect: string | null;
  serves: string;
  subService: string | null;
};
type FactsUnit = {
  kind: string;
  port: number | null;
  healthPath: string | null;
  healthExpect: string | null;
};
type Facts = { services: Record<string, FactsService>; units: Record<string, FactsUnit> };

/** Units that must never get an HTTP probe row, with the reason. */
const SKIP_UNITS: Record<string, string> = {
  pulse: "ENet/UDP protocol, no HTTP surface",
  "catalyrst-sites": "the /server page itself \u{2014} if it renders, sites is up",
};

/** Display strings the topology source does not carry. Fallback: the key. */
const DISPLAY: Record<string, { name: string; serves?: string }> = {
  content: {
    name: "Content + Lambdas",
    serves: "catalyst content/lambdas API \u{2014} nearly every other surface reads through it",
  },
  "explorer-api": { name: "Explorer API" },
  archipelago: { name: "Archipelago" },
  explore: {
    name: "Explore bundle",
    serves: "places, events, map tiles, worlds, lists",
  },
  create: { name: "Create bundle", serves: "builder, camera reel, asset-bundle registry" },
  social: {
    name: "Social bundle",
    serves: "communities, comms gatekeeper, notifications, badges, media",
  },
  data: {
    name: "Data bundle",
    serves: "marketplace catalog, economy, prices, credits, JSON-RPC proxy",
  },
  abgen: { name: "Asset bundles" },
  "social-rpc": { name: "Social RPC" },
  telemetry: { name: "Telemetry" },
  governance: { name: "Governance" },
  presence: { name: "Presence" },
  "world-storage": { name: "World storage" },
  signatures: { name: "Signatures" },
  "profile-images": { name: "Profile images" },
  "opensea-resolver": { name: "OpenSea resolver" },
  "scene-state": { name: "Scene state" },
  "livekit-signaling": {
    name: "LiveKit SFU",
    serves: "voice + comms media transport (signaling; media rides UDP 7882)",
  },
  libretranslate: {
    name: "LibreTranslate",
    serves: "chat translation backend (en/es models, loopback-only)",
  },
  "nats-monitor": {
    name: "NATS",
    serves: "message bus between archipelago peers (monitoring port)",
  },
  "squid-eth": { name: "Squid indexer (Ethereum)" },
  "squid-polygon": { name: "Squid indexer (Polygon)" },
};

function readFacts(): Facts {
  const raw = execFileSync(
    "nix",
    ["eval", "--extra-experimental-features", "nix-command", "--file", FACTS, "--json"],
    { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"] },
  );
  return JSON.parse(raw) as Facts;
}

type Row = {
  key: string;
  name: string;
  unit: string;
  port: number;
  healthPath: string;
  expect: string;
  serves: string;
  members?: string[];
};

function buildRows(facts: Facts): Row[] {
  const byUnit = new Map<string, [string, FactsService][]>();
  for (const [key, svc] of Object.entries(facts.services)) {
    const list = byUnit.get(svc.unit) ?? [];
    list.push([key, svc]);
    byUnit.set(svc.unit, list);
  }

  const rows: Row[] = [];
  for (const [unitName, unit] of Object.entries(facts.units)) {
    if (unit.kind !== "long-running") continue;
    if (unitName in SKIP_UNITS) continue;
    const carried = (byUnit.get(unitName) ?? []).sort(([a], [b]) => a.localeCompare(b));
    const primary = carried.find(([, s]) => s.healthPath !== null);
    const alias = unitName.replace(/^catalyrst-/, "");

    let key: string;
    let port: number;
    let healthPath: string;
    let expect: string;
    if (primary) {
      key = primary[0];
      port = primary[1].port;
      healthPath = primary[1].healthPath as string;
      expect = primary[1].healthExpect ?? "2xx";
    } else if (unit.healthPath !== null && unit.port !== null) {
      key = alias;
      port = unit.port;
      healthPath = unit.healthPath;
      expect = unit.healthExpect ?? "2xx";
    } else {
      fail(
        `unit ${unitName} is long-running but declares no health endpoint \u{2014} declare healthPath in facts.nix or add it to SKIP_UNITS with a reason`,
      );
    }

    const memberKeys = carried.map(([k]) => k);
    const members =
      memberKeys.length > 1 || (memberKeys.length === 1 && memberKeys[0] !== key)
        ? memberKeys
        : undefined;

    const display = DISPLAY[key];
    const serves =
      display?.serves ??
      (primary ? primary[1].serves : carried[0]?.[1].serves) ??
      memberKeys.join(", ");
    rows.push({
      key,
      name: display?.name ?? key,
      unit: unitName,
      port,
      healthPath,
      expect,
      serves,
      ...(members ? { members } : {}),
    });
  }
  return rows.sort((a, b) => a.port - b.port);
}

function render(rows: Row[]): string {
  const body = rows
    .map((r) => {
      const members = r.members
        ? `\n    members: [${r.members.map((m) => JSON.stringify(m)).join(", ")}],`
        : "";
      return `  {
    key: ${JSON.stringify(r.key)},
    name: ${JSON.stringify(r.name)},
    unit: ${JSON.stringify(r.unit)},
    port: ${r.port},
    healthPath: ${JSON.stringify(r.healthPath)},
    expect: ${JSON.stringify(r.expect)},
    serves: ${JSON.stringify(r.serves)},${members}
  },`;
    })
    .join("\n");
  return `// Generated by scripts/gen-operator-registry.mts from nixos/facts.nix \u{2014} do not edit.
// Regenerate: npm run gen:operator \u{2014} verify drift: npm run gen:operator:check
import type { OperatorService } from "./registry";

export const SERVICES: OperatorService[] = [
${body}
];
`;
}

const facts = readFacts();
const next = render(buildRows(facts));

if (process.argv.includes("--check")) {
  let current: string;
  try {
    current = readFileSync(OUT, "utf8");
  } catch {
    fail(`${OUT} is missing \u{2014} run npm run gen:operator`);
  }
  if (current !== next) {
    fail("services.generated.ts drifted from nixos/facts.nix \u{2014} run npm run gen:operator");
  }
  console.log("gen-operator-registry: in sync with facts.nix");
} else {
  writeFileSync(OUT, next);
  console.log(`gen-operator-registry: wrote ${OUT}`);
}
