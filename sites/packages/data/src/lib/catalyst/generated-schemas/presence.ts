// GENERATED from catalyrst/ui3/src/generated/catalyst/presence by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { CurrentSnapshot } from "@ui/generated/catalyst/presence/CurrentSnapshot";
import type { SceneOccupancyRow } from "@ui/generated/catalyst/presence/SceneOccupancyRow";
import type { WorldHeadcountRow } from "@ui/generated/catalyst/presence/WorldHeadcountRow";

export const CurrentSnapshotSchema = z.object({
  snapshot_id: z.number(),
  taken_at: z.string(),
  peers_count: z.number(),
  islands_count: z.number(),
  hot_scenes_count: z.number(),
  scenes_polled: z.number(),
  scene_users_total: z.number(),
  worlds_polled: z.number(),
  active_worlds: z.number(),
  world_users_total: z.number(),
  worlds_live_total: z.number().nullable(),
});

export const SceneOccupancyRowSchema = z.object({
  taken_at: z.string(),
  pointer: z.string(),
  scene_name: z.string().nullable(),
  realm: z.string(),
  count: z.number(),
});

export const WorldHeadcountRowSchema = z.object({
  taken_at: z.string(),
  world_name: z.string(),
  count: z.number(),
  live_users: z.number().nullable(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertCurrentSnapshot = Assert<Mutual<CurrentSnapshot, z.infer<typeof CurrentSnapshotSchema>>>;
export type _AssertSceneOccupancyRow = Assert<Mutual<SceneOccupancyRow, z.infer<typeof SceneOccupancyRowSchema>>>;
export type _AssertWorldHeadcountRow = Assert<Mutual<WorldHeadcountRow, z.infer<typeof WorldHeadcountRowSchema>>>;
