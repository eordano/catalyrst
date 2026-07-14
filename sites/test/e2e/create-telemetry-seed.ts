import type { Pool } from "pg";

type ArmSpec = {
  variant: string;
  exposures: number;
  conversions: number;
  verdict: "SHIP" | "KILL" | "KEEP RUNNING";
};

export const CREATE_FIXTURE = {
  expKey: "create_entry_preview",
  metric: "create_preview_rate",
  baseEvent: "create_preview",
  exposureEvent: "experiment_exposed",
  story: "create/entry-preview",
  control: { variant: "control", exposures: 6000, conversions: 1500 } as const,
  treatments: [
    { variant: "builder-or-download", exposures: 6000, conversions: 1980, verdict: "SHIP" },
    { variant: "capability-routed", exposures: 6000, conversions: 1740, verdict: "SHIP" },
    { variant: "download-hub", exposures: 6000, conversions: 1020, verdict: "KILL" },
    { variant: "hub-or-download", exposures: 6000, conversions: 1560, verdict: "KEEP RUNNING" },
  ] as ArmSpec[],
} as const;

const RECEIVED_AT = "2026-06-20T12:00:00Z";

function trackBody(
  event: string,
  variant: string,
  anonymousId: string,
): Record<string, unknown> {
  return {
    type: "track",
    event,
    anonymousId,
    properties: {
      variant,
      exp_key: CREATE_FIXTURE.expKey,
      story: CREATE_FIXTURE.story,
    },
  };
}

async function seedArm(
  pool: Pool,
  arm: { variant: string; exposures: number; conversions: number },
): Promise<number> {
  const bodies: string[] = [];
  for (let i = 0; i < arm.exposures; i++) {
    const sid = `cep-${arm.variant}-${i}`;
    bodies.push(JSON.stringify(trackBody(CREATE_FIXTURE.exposureEvent, arm.variant, sid)));
    if (i < arm.conversions) {
      bodies.push(JSON.stringify(trackBody(CREATE_FIXTURE.baseEvent, arm.variant, sid)));
    }
  }
  const CHUNK = 1000;
  for (let off = 0; off < bodies.length; off += CHUNK) {
    const slice = bodies.slice(off, off + CHUNK);
    const values = slice
      .map((_b, j) => `('segment','sites','track','${RECEIVED_AT}'::timestamptz, $${j + 1}::jsonb)`)
      .join(",");
    await pool.query(
      `INSERT INTO telemetry.telemetry_events (source, project, event_kind, received_at, body)
       VALUES ${values}`,
      slice,
    );
  }
  return bodies.length;
}

export async function seedCreateEntryPreview(pool: Pool): Promise<number> {
  let n = 0;
  n += await seedArm(pool, CREATE_FIXTURE.control);
  for (const t of CREATE_FIXTURE.treatments) n += await seedArm(pool, t);
  return n;
}
