// GENERATED from catalyrst/ui3/src/generated/catalyst/comms by catalyrst/sites/scripts/gen-zod-schemas.mts. Do not edit.
import { z } from "zod";

import type { BanStatus } from "@ui/generated/catalyst/comms/BanStatus";
import type { PublicUserBan } from "@ui/generated/catalyst/comms/PublicUserBan";
import type { UserBan } from "@ui/generated/catalyst/comms/UserBan";
import type { UserWarning } from "@ui/generated/catalyst/comms/UserWarning";

export const PublicUserBanSchema = z.object({
  id: z.string(),
  bannedAddress: z.string(),
  bannedBy: z.string(),
  reason: z.string(),
  customMessage: z.string().nullable(),
  bannedAt: z.string(),
  expiresAt: z.string().nullable(),
  liftedAt: z.string().nullable(),
  liftedBy: z.string().nullable(),
  createdAt: z.string(),
});

export const BanStatusSchema = z.object({
  isBanned: z.boolean(),
  ban: PublicUserBanSchema.optional(),
});

export const UserBanSchema = z.object({
  id: z.string(),
  bannedAddress: z.string(),
  bannedBy: z.string(),
  reason: z.string(),
  customMessage: z.string().nullable(),
  bannedDeviceId: z.string().nullable(),
  bannedAt: z.string(),
  expiresAt: z.string().nullable(),
  liftedAt: z.string().nullable(),
  liftedBy: z.string().nullable(),
  createdAt: z.string(),
});

export const UserWarningSchema = z.object({
  id: z.string(),
  warnedAddress: z.string(),
  warnedBy: z.string(),
  reason: z.string(),
  warnedAt: z.string(),
  createdAt: z.string(),
});

type AssignableTo<Sub, Sup> = Sub extends Sup ? true : false;
type Mutual<A, B> = AssignableTo<A, B> extends true ? AssignableTo<B, A> : false;
type Assert<T extends true> = T;

export type _AssertBanStatus = Assert<Mutual<BanStatus, z.infer<typeof BanStatusSchema>>>;
export type _AssertPublicUserBan = Assert<Mutual<PublicUserBan, z.infer<typeof PublicUserBanSchema>>>;
export type _AssertUserBan = Assert<Mutual<UserBan, z.infer<typeof UserBanSchema>>>;
export type _AssertUserWarning = Assert<Mutual<UserWarning, z.infer<typeof UserWarningSchema>>>;
