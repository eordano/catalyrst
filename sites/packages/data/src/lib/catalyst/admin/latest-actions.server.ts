import { z } from "zod";

import { getJSON } from "../client";
import {
  type ControlResult,
  available,
  unavailable,
} from "./availability";

export type DeployAction = {
  entityType: string;
  entityId: string;
  deployer: string;
  at: string;
};

const DeploymentSchema = z.object({
  entityType: z.string(),
  entityId: z.string(),
  deployedBy: z.string(),
  localTimestamp: z.coerce.number(),
});

const ResponseSchema = z.object({
  deployments: z.array(DeploymentSchema),
});

const SERVER_CHECK =
  "catalyrst-server/src/routes.rs:85 (GET /content/deployments, public)";

export async function loadRecentDeployments(
  limit = 20,
  signal?: AbortSignal,
): Promise<ControlResult<DeployAction[]>> {
  let raw: unknown;
  try {
    raw = await getJSON<unknown>("/content/deployments", {
      signal,
      query: {
        limit,
        sortingField: "local_timestamp",
        sortingOrder: "DESC",
      },
    });
  } catch {
    return unavailable(
      "backend-error",
      "The content server did not answer the recent-deployments query.",
      { serverCheck: SERVER_CHECK },
    );
  }

  const parsed = ResponseSchema.safeParse(raw);
  if (!parsed.success) {
    return unavailable(
      "backend-error",
      "The deployments feed returned an unexpected shape.",
      { serverCheck: SERVER_CHECK },
    );
  }

  const rows: DeployAction[] = parsed.data.deployments.map((d) => ({
    entityType: d.entityType,
    entityId: d.entityId,
    deployer: d.deployedBy,
    at: new Date(d.localTimestamp).toISOString(),
  }));

  return available(rows);
}
