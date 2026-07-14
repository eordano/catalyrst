import type { AuthIdentity } from "../../auth/types";
import { getJSON, type GetOptions } from "../client";
import {
  buildSceneEntity,
  deployScene,
  type DeployOptions,
  type DeployResult,
  type PreparedDeployment,
} from "./deploy-scene";

const PARCEL_RE = /^-?\d+,-?\d+$/;

export type SceneDeletionInput = {
  pointers: string[];
  base?: string;
  timestamp?: number;
};

export function buildTombstoneSceneJson(
  pointers: string[],
  base: string,
  timestamp: number,
): Record<string, unknown> {
  return {
    main: "bin/empty.js",
    scene: { base, parcels: pointers },
    display: { title: "Deleted scene" },
    dclDeleted: true,
    dclDeletedAt: timestamp,
  };
}

function assertPointers(pointers: string[]): void {
  if (pointers.length === 0) {
    throw new Error("deleteScene: at least one pointer is required");
  }
  for (const p of pointers) {
    if (!PARCEL_RE.test(p.trim())) {
      throw new Error(
        `deleteScene: pointer "${p}" is not a parcel "x,y" \u{2014} refusing to deploy ` +
          "a tombstone over a non-parcel pointer",
      );
    }
  }
}

export async function buildSceneDeletion(
  input: SceneDeletionInput,
): Promise<PreparedDeployment> {
  const pointers = input.pointers.map((p) => p.trim());
  assertPointers(pointers);
  const base = (input.base ?? pointers[0]).trim();
  const timestamp = input.timestamp ?? Date.now();

  const metadata = buildTombstoneSceneJson(pointers, base, timestamp);
  const sceneJson = new TextEncoder().encode(JSON.stringify(metadata));

  return buildSceneEntity({
    pointers,
    files: [{ file: "scene.json", content: sceneJson }],
    metadata,
    timestamp,
  });
}

export type ActiveScene = {
  id: string;
  pointers: string[];
  timestamp: number;
};

export async function resolveActiveScene(
  pointer: string,
  opts: GetOptions = {},
): Promise<ActiveScene | null> {
  const list = await getJSON<unknown[]>("/content/entities/scene", {
    ...opts,
    query: { ...(opts.query ?? {}), pointer },
  });
  const first = Array.isArray(list) ? list[0] : null;
  if (!first || typeof first !== "object") return null;
  const e = first as Record<string, unknown>;
  if (typeof e.id !== "string" || !Array.isArray(e.pointers)) return null;
  return {
    id: e.id,
    pointers: e.pointers.map((p) => String(p)),
    timestamp: typeof e.timestamp === "number" ? e.timestamp : 0,
  };
}

export type DeleteSceneResult =
  | { ok: true; status: number; tombstoneId: string; overrode: string[] }
  | { ok: false; status: number; errors: string[] };

export type DeleteSceneOptions = DeployOptions & {
  expectedOwner?: string;
};

export async function deleteScene(
  identity: AuthIdentity,
  input: SceneDeletionInput,
  opts: DeleteSceneOptions = {},
): Promise<DeleteSceneResult> {
  if (opts.expectedOwner) {
    const owner = opts.expectedOwner.toLowerCase();
    if (identity.signer.toLowerCase() !== owner) {
      return {
        ok: false,
        status: 0,
        errors: [
          `Refusing to delete: connected wallet ${identity.signer} is not the ` +
            `scene owner ${owner}.`,
        ],
      };
    }
  }

  let prepared: PreparedDeployment;
  try {
    prepared = await buildSceneDeletion(input);
  } catch (err) {
    return { ok: false, status: 0, errors: [(err as Error).message] };
  }

  const result: DeployResult = await deployScene(identity, prepared, opts);
  if (result.ok) {
    return {
      ok: true,
      status: result.status,
      tombstoneId: prepared.entityId,
      overrode: input.pointers.map((p) => p.trim()),
    };
  }
  return { ok: false, status: result.status, errors: result.errors };
}
