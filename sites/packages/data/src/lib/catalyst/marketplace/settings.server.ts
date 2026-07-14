import { z } from "zod";

import { catalystBase } from "../client";
import { track } from "@core/lib/telemetry/track";
import {
  StoreEntitySchema,
  emptyStore,
  getStoreUrn,
  storeFromEntity,
  type Store,
} from "./settings";

/**
 * "catalyst" -- `store` is what this node holds for the address.
 * "empty" -- the node answered and the address has published no store.
 * "unavailable" -- the read failed. `store` is blank because we know nothing,
 *   so the editor must not present those blanks as the seller's current
 *   settings -- saving them would erase a store we simply could not read.
 */
export type StoreResult = {
  store: Store;
  source: "catalyst" | "empty" | "unavailable";
  reason?: string;
};

function message(error: unknown): string {
  return error instanceof Error ? error.message : String(error);
}

const ActiveEntitiesSchema = z.array(z.object({ metadata: z.unknown() }));

export async function loadStore(
  address: string,
  opts: { base?: string; signal?: AbortSignal } = {},
): Promise<StoreResult> {
  const base = catalystBase(opts.base);
  const blank = emptyStore(address);

  let raw: unknown;
  try {
    const res = await fetch(`${base}/content/entities/active`, {
      method: "POST",
      headers: { "content-type": "application/json", accept: "application/json" },
      body: JSON.stringify({ pointers: [getStoreUrn(address)] }),
      signal: opts.signal,
    });
    if (!res.ok) {
      return {
        store: blank,
        source: "unavailable",
        reason: `the content server answered ${res.status}`,
      };
    }
    raw = await res.json();
  } catch (error) {
    return { store: blank, source: "unavailable", reason: message(error) };
  }

  const arr = ActiveEntitiesSchema.safeParse(raw);
  let meta: unknown;
  if (arr.success) {
    meta = arr.data[0]?.metadata;
  } else {
    console.warn(
      "[catalyst] entities/active (store) failed schema validation",
      arr.error.issues,
    );
    track(
      "catalyst_schema_drift",
      {
        module: "marketplace/settings",
        path: "/content/entities/active",
        issues: arr.error.issues
          .slice(0, 5)
          .map((i) => `${i.path.map(String).join(".")}:${i.code}`),
      },
      { sid: "schema-drift" },
    );
    const first = Array.isArray(raw) ? (raw as unknown[])[0] : undefined;
    meta =
      first && typeof first === "object"
        ? (first as { metadata?: unknown }).metadata
        : undefined;
  }

  if (meta == null) {
    if (arr.success) return { store: blank, source: "empty" };
    return {
      store: blank,
      source: "unavailable",
      reason: "the content server returned an unexpected shape",
    };
  }

  const parsed = StoreEntitySchema.safeParse(meta);
  if (!parsed.success) {
    return {
      store: blank,
      source: "unavailable",
      reason: "the stored settings did not match the expected shape",
    };
  }
  return { store: storeFromEntity(parsed.data, base), source: "catalyst" };
}
