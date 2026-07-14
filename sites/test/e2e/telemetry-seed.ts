import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import type { Pool } from "pg";

const FUNCTIONS_SQL = fileURLToPath(
  new URL("../../../../deploy/metabase/functions.sql", import.meta.url),
);

export const FIXTURE = {
  expKey: "places_layout_v2",
  metric: "jump_in_rate",
  baseEvent: "jump_in",
  exposureEvent: "experiment_exposed",
  control: { variant: "control", exposures: 400, conversions: 80 },
  treatment: { variant: "treatment", exposures: 400, conversions: 140 },
} as const;

export async function createTelemetrySchema(pool: Pool): Promise<void> {
  await pool.query(`
    CREATE SCHEMA IF NOT EXISTS telemetry;
    CREATE TABLE IF NOT EXISTS telemetry.telemetry_events (
      id          BIGSERIAL PRIMARY KEY,
      source      TEXT NOT NULL,
      project     TEXT NOT NULL DEFAULT '',
      event_kind  TEXT NOT NULL DEFAULT '',
      received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
      body        JSONB NOT NULL
    );
  `);
}

export async function applyFunctions(pool: Pool): Promise<void> {
  await pool.query(readFileSync(FUNCTIONS_SQL, "utf8"));
}

function trackBody(
  event: string,
  variant: string,
  anonymousId: string,
): Record<string, unknown> {
  return {
    type: "track",
    event,
    anonymousId,
    properties: { variant, exp_key: FIXTURE.expKey },
  };
}

type ArmSpec = { variant: string; exposures: number; conversions: number };

async function seedArm(pool: Pool, arm: ArmSpec): Promise<number> {
  const rows: Array<Record<string, unknown>> = [];
  for (let i = 0; i < arm.exposures; i++) {
    const sid = `${arm.variant}-${i}`;
    rows.push(trackBody(FIXTURE.exposureEvent, arm.variant, sid));
    if (i < arm.conversions) {
      rows.push(trackBody(FIXTURE.baseEvent, arm.variant, sid));
    }
  }
  const RECEIVED_AT = "2026-06-20T12:00:00Z";
  for (const body of rows) {
    await pool.query(
      `INSERT INTO telemetry.telemetry_events (source, project, event_kind, received_at, body)
       VALUES ('segment', 'sites', 'track', $1::timestamptz, $2::jsonb)`,
      [RECEIVED_AT, JSON.stringify(body)],
    );
  }
  return rows.length;
}

export async function seedTelemetry(pool: Pool): Promise<number> {
  await pool.query("TRUNCATE telemetry.telemetry_events RESTART IDENTITY");
  let n = 0;
  n += await seedArm(pool, FIXTURE.control);
  n += await seedArm(pool, FIXTURE.treatment);
  return n;
}

export async function bootstrapTelemetry(pool: Pool): Promise<number> {
  await createTelemetrySchema(pool);
  await applyFunctions(pool);
  return seedTelemetry(pool);
}
