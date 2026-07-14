import { Pool } from "pg";
import { z } from "zod";

import { OPERATOR_EVENTS } from "@core/lib/telemetry/operator-events";

export type MetricsSource = "live" | "unavailable";

export type DeployFunnel = {
  stages: { event: string; label: string; count: number }[];
  conversion: number;
  source: MetricsSource;
};

export type AdminActivity = {
  rows: { event: string; label: string; count: number }[];
  source: MetricsSource;
};

export type OperatorMetrics = { funnel: DeployFunnel; admin: AdminActivity };

const FUNNEL_STAGES: { event: string; label: string }[] = [
  { event: OPERATOR_EVENTS.deployStarted, label: "Deploy started" },
  {
    event: OPERATOR_EVENTS.placementValidated,
    label: "Placement / name validated",
  },
  {
    event: OPERATOR_EVENTS.placementRejected,
    label: "Placement rejected (guardrail)",
  },
  { event: OPERATOR_EVENTS.deployCompleted, label: "Deploy completed" },
];

const ADMIN_ROWS: { event: string; label: string }[] = [
  { event: OPERATOR_EVENTS.sceneAdminOpened, label: "Scene-admin opened" },
  { event: OPERATOR_EVENTS.adminChanged, label: "Admin changed" },
  { event: OPERATOR_EVENTS.banIssued, label: "Bans issued" },
];

const TRACKED_EVENTS = [...FUNNEL_STAGES, ...ADMIN_ROWS].map((x) => x.event);

function connectionString(): string | undefined {
  const cs = process.env.TELEMETRY_DATABASE_URL;
  return cs && cs.trim() !== "" ? cs : undefined;
}

let pool: Pool | null = null;
function getPool(): Pool | null {
  const cs = connectionString();
  if (!cs) return null;
  if (!pool) pool = new Pool({ connectionString: cs, max: 2 });
  return pool;
}

const COUNTS_SQL = `SELECT body->>'event' AS event, count(*)::int AS n
   FROM telemetry.telemetry_events
   WHERE source = 'segment' AND body->>'event' = ANY($1::text[])
   GROUP BY 1`;

const CountRowSchema = z.object({ event: z.string(), n: z.coerce.number() });

async function operatorEventCounts(): Promise<Map<string, number> | null> {
  const db = getPool();
  if (!db) return null;
  try {
    const res = await db.query(COUNTS_SQL, [TRACKED_EVENTS]);
    const counts = new Map<string, number>();
    for (const raw of res.rows) {
      const row = CountRowSchema.safeParse(raw);
      if (!row.success) {
        console.warn(
          "[operator-metrics] telemetry count row failed schema validation",
          row.error.issues,
        );
        continue;
      }
      counts.set(row.data.event, row.data.n);
    }
    return counts;
  } catch {
    return null;
  }
}

function emptyMetrics(source: MetricsSource): OperatorMetrics {
  return {
    funnel: {
      stages: FUNNEL_STAGES.map((s) => ({ ...s, count: 0 })),
      conversion: 0,
      source,
    },
    admin: { rows: ADMIN_ROWS.map((r) => ({ ...r, count: 0 })), source },
  };
}

export async function loadOperatorMetrics(): Promise<OperatorMetrics> {
  const counts = await operatorEventCounts();
  if (!counts) return emptyMetrics("unavailable");

  const get = (event: string) => counts.get(event) ?? 0;
  const stages = FUNNEL_STAGES.map((s) => ({ ...s, count: get(s.event) }));
  const started = stages[0].count;
  const completed = get(OPERATOR_EVENTS.deployCompleted);

  return {
    funnel: {
      stages,
      conversion: started > 0 ? completed / started : 0,
      source: "live",
    },
    admin: {
      rows: ADMIN_ROWS.map((r) => ({ ...r, count: get(r.event) })),
      source: "live",
    },
  };
}
