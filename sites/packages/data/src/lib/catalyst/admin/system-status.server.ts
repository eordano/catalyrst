import { readFile } from "node:fs/promises";

import { z } from "zod";

import {
  type ControlResult,
  available,
  unavailable,
} from "./availability";

const STALE_AFTER_MS = 3 * 60_000;

const UnitSchema = z.object({
  unit: z.string(),
  active_state: z.string(),
  sub_state: z.string(),
  n_restarts: z.coerce.number(),
  active_since: z.string(),
});

const ProbeSchema = z.object({
  name: z.string(),
  url: z.string(),
  http_status: z.coerce.number(),
  ok: z.boolean(),
});

const SnapshotSchema = z.object({
  collected_at: z.string(),
  units: z.array(UnitSchema),
  probes: z.array(ProbeSchema),
});

export type SystemUnit = z.infer<typeof UnitSchema>;
export type SystemProbe = z.infer<typeof ProbeSchema>;

export type SystemLink = {
  label: string;
  href: string;
  scope: "public" | "operator";
};

export type SystemStatus = {
  collectedAt: string;
  stale: boolean;
  units: SystemUnit[];
  probes: SystemProbe[];
};

/**
 * Deployed surfaces worth a click from the operator. These are stable public
 * routes, not derived from the live probe set, so the list renders even when
 * the collector has never run. Live up/down for each comes from `probes`
 * (keyed by name) and the systemd `units`; this is only the address book.
 */
const LINKS: SystemLink[] = [
  { label: "Catalyst \u{2014} /about", href: "/about", scope: "public" },
  { label: "Marketplace", href: "/marketplace", scope: "public" },
  { label: "Places", href: "/places", scope: "public" },
  { label: "Events / What's On", href: "/events", scope: "public" },
  { label: "Governance", href: "/governance", scope: "public" },
  { label: "Play", href: "/play", scope: "public" },
  { label: "Worlds", href: "https://worlds.example.com", scope: "public" },
  { label: "Telemetry dashboard", href: "/telemetry", scope: "operator" },
  { label: "Metabase (experiments)", href: "/metabase", scope: "operator" },
  { label: "Forge (code mirror)", href: "https://forge.catalyst.example.com", scope: "operator" },
  { label: "Lorebook", href: "https://lore.catalyst.example.com", scope: "operator" },
  { label: "Code corpus", href: "https://code.catalyst.example.com", scope: "operator" },
];

export function deployedSurfaces(): SystemLink[] {
  return LINKS;
}

function statusPath(): string | undefined {
  const p = process.env.SYSTEM_STATUS_FILE;
  return p && p.trim() !== "" ? p.trim() : undefined;
}

export async function loadSystemStatus(): Promise<ControlResult<SystemStatus>> {
  const path = statusPath();
  if (!path) {
    return unavailable(
      "not-configured",
      "System-status collector is not wired: set SYSTEM_STATUS_FILE and install deploy-system-status.timer.",
      { serverCheck: "deploy/systemd/deploy-system-status.service" },
    );
  }

  let raw: string;
  try {
    raw = await readFile(path, "utf8");
  } catch {
    return unavailable(
      "not-wired",
      "System-status snapshot has not been produced yet; the collector timer may not be running.",
      { serverCheck: "deploy/scripts/collect-system-status.sh", fix: "systemctl --user start deploy-system-status.timer" },
    );
  }

  const parsed = SnapshotSchema.safeParse(JSON.parse(raw));
  if (!parsed.success) {
    return unavailable("backend-error", "System-status snapshot is malformed.", {
      serverCheck: "deploy/scripts/collect-system-status.sh",
    });
  }

  const collectedMs = Date.parse(parsed.data.collected_at);
  const stale =
    Number.isFinite(collectedMs) && Date.now() - collectedMs > STALE_AFTER_MS;

  return available({
    collectedAt: parsed.data.collected_at,
    stale,
    units: [...parsed.data.units].sort((a, b) => a.unit.localeCompare(b.unit)),
    probes: parsed.data.probes,
  });
}
