// GENERATED from catalyrst/ui3/src/generated/catalyst/worlds by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { FederationMirrorResponse } from "@ui/generated/catalyst/worlds/FederationMirrorResponse";
import type { FederationPeerOmissionView } from "@ui/generated/catalyst/worlds/FederationPeerOmissionView";
import type { FederationPeersResponse } from "@ui/generated/catalyst/worlds/FederationPeersResponse";
import type { FederationPeerStatusLine } from "@ui/generated/catalyst/worlds/FederationPeerStatusLine";
import type { FederationPeerStatusView } from "@ui/generated/catalyst/worlds/FederationPeerStatusView";
import type { FederationPeerView } from "@ui/generated/catalyst/worlds/FederationPeerView";
import type { FederationRefreshPeerResult } from "@ui/generated/catalyst/worlds/FederationRefreshPeerResult";
import type { FederationRefreshResponse } from "@ui/generated/catalyst/worlds/FederationRefreshResponse";
import type { LiveDataPayload } from "@ui/generated/catalyst/worlds/LiveDataPayload";
import type { LiveDataResponse } from "@ui/generated/catalyst/worlds/LiveDataResponse";
import type { RemoteWorldView } from "@ui/generated/catalyst/worlds/RemoteWorldView";
import type { SetMirrorHiddenRequest } from "@ui/generated/catalyst/worlds/SetMirrorHiddenRequest";
import type { SetMirrorHiddenResponse } from "@ui/generated/catalyst/worlds/SetMirrorHiddenResponse";
import type { WorldOccupancy } from "@ui/generated/catalyst/worlds/WorldOccupancy";

export const FederationPeerStatusViewSchema = z.object({
  lastAttemptAt: z.string().nullable(),
  lastSuccessAt: z.string().nullable(),
  lastError: z.string().nullable(),
  worldsObserved: z.number(),
  entriesSkipped: z.number(),
  truncated: z.boolean(),
  hasEverSucceeded: z.boolean(),
});

export const FederationPeerStatusLineSchema = z.object({
  peerId: z.string(),
  status: FederationPeerStatusViewSchema,
});

export const RemoteWorldViewSchema = z.object({
  peerId: z.string(),
  name: z.string(),
  title: z.string().nullable(),
  description: z.string().nullable(),
  contentRating: z.string().nullable(),
  categories: z.array(z.string()),
  thumbnailHash: z.string().nullable(),
  deployedScenes: z.number(),
  lastDeployedAt: z.string().nullable(),
  observedAt: z.string(),
});

export const FederationMirrorResponseSchema = z.object({
  worlds: z.array(RemoteWorldViewSchema),
  total: z.number(),
  peers: z.array(FederationPeerStatusLineSchema),
});

export const FederationPeerOmissionViewSchema = z.object({
  peerId: z.string(),
  reason: z.string(),
  detail: z.string(),
});

export const FederationPeerViewSchema = z.object({
  peerId: z.string(),
  worldsUrl: z.string(),
  daoProposal: z.string(),
  addedAt: z.string(),
  insecureLoopback: z.boolean(),
  status: FederationPeerStatusViewSchema,
});

export const FederationPeersResponseSchema = z.object({
  configured: z.boolean(),
  peersFile: z.string(),
  peers: z.array(FederationPeerViewSchema),
  omitted: z.array(FederationPeerOmissionViewSchema),
});

export const FederationRefreshPeerResultSchema = z.object({
  peerId: z.string(),
  ok: z.boolean(),
  worldsObserved: z.number(),
  entriesSkipped: z.number(),
  truncated: z.boolean(),
  localNameCollisions: z.array(z.string()).nullable(),
  localNameCollisionsError: z.string().nullable(),
  error: z.string().nullable(),
});

export const FederationRefreshResponseSchema = z.object({
  polled: z.array(FederationRefreshPeerResultSchema),
});

export const WorldOccupancySchema = z.object({
  worldName: z.string(),
  users: z.number(),
});

export const LiveDataPayloadSchema = z.object({
  totalUsers: z.number(),
  perWorld: z.array(WorldOccupancySchema),
});

export const LiveDataResponseSchema = z.object({
  data: LiveDataPayloadSchema,
  lastUpdated: z.string(),
});

export const SetMirrorHiddenRequestSchema = z.object({
  hidden: z.boolean(),
});

export const SetMirrorHiddenResponseSchema = z.object({
  peerId: z.string(),
  worldName: z.string(),
  hidden: z.boolean(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertFederationMirrorResponse = Assert<Mutual<FederationMirrorResponse, z.infer<typeof FederationMirrorResponseSchema>>>;
export type _AssertFederationPeerOmissionView = Assert<Mutual<FederationPeerOmissionView, z.infer<typeof FederationPeerOmissionViewSchema>>>;
export type _AssertFederationPeersResponse = Assert<Mutual<FederationPeersResponse, z.infer<typeof FederationPeersResponseSchema>>>;
export type _AssertFederationPeerStatusLine = Assert<Mutual<FederationPeerStatusLine, z.infer<typeof FederationPeerStatusLineSchema>>>;
export type _AssertFederationPeerStatusView = Assert<Mutual<FederationPeerStatusView, z.infer<typeof FederationPeerStatusViewSchema>>>;
export type _AssertFederationPeerView = Assert<Mutual<FederationPeerView, z.infer<typeof FederationPeerViewSchema>>>;
export type _AssertFederationRefreshPeerResult = Assert<Mutual<FederationRefreshPeerResult, z.infer<typeof FederationRefreshPeerResultSchema>>>;
export type _AssertFederationRefreshResponse = Assert<Mutual<FederationRefreshResponse, z.infer<typeof FederationRefreshResponseSchema>>>;
export type _AssertLiveDataPayload = Assert<Mutual<LiveDataPayload, z.infer<typeof LiveDataPayloadSchema>>>;
export type _AssertLiveDataResponse = Assert<Mutual<LiveDataResponse, z.infer<typeof LiveDataResponseSchema>>>;
export type _AssertRemoteWorldView = Assert<Mutual<RemoteWorldView, z.infer<typeof RemoteWorldViewSchema>>>;
export type _AssertSetMirrorHiddenRequest = Assert<Mutual<SetMirrorHiddenRequest, z.infer<typeof SetMirrorHiddenRequestSchema>>>;
export type _AssertSetMirrorHiddenResponse = Assert<Mutual<SetMirrorHiddenResponse, z.infer<typeof SetMirrorHiddenResponseSchema>>>;
export type _AssertWorldOccupancy = Assert<Mutual<WorldOccupancy, z.infer<typeof WorldOccupancySchema>>>;
