import { z } from "zod";

import { catalystBase, getJSON, postJSON, signedGetJSON } from "../client";
import type { GetOptions } from "../client";
import { signedFetch } from "../../auth/signer";
import type { AuthIdentity } from "../../auth/types";

import {
  ClaimCreditsResponseSchema,
  CreditsProgramProgressResponseSchema,
  GoalDataSchema,
  GoalProgressDataSchema,
  SeasonDataSchema,
  SeasonsDataSchema,
  WeekSchema,
} from "../generated-schemas/credits";

export {
  SeasonDataSchema,
  WeekSchema,
  SeasonsDataSchema,
  GoalProgressDataSchema as GoalProgressSchema,
  GoalDataSchema as GoalSchema,
  CreditsProgramProgressResponseSchema as ProgressSchema,
  ClaimCreditsResponseSchema as ClaimResultSchema,
};

export type SeasonData = z.infer<typeof SeasonDataSchema>;
export type Week = z.infer<typeof WeekSchema>;
export type SeasonsData = z.infer<typeof SeasonsDataSchema>;
export type Goal = z.infer<typeof GoalDataSchema>;
export type CreditsProgress = z.infer<typeof CreditsProgramProgressResponseSchema>;
export type ClaimResult = z.infer<typeof ClaimCreditsResponseSchema>;

export async function fetchSeasons(opts: GetOptions = {}): Promise<SeasonsData> {
  const raw = await getJSON<unknown>("/credits/seasons", opts);
  return SeasonsDataSchema.parse(raw);
}

export async function fetchProgressSigned(
  identity: AuthIdentity,
  wallet: string,
  opts: { signal?: AbortSignal } = {},
): Promise<CreditsProgress> {
  const raw = await signedGetJSON<unknown>(
    `/credits/users/${encodeURIComponent(wallet)}/progress`,
    { identity, ...opts },
  );
  return CreditsProgramProgressResponseSchema.parse(raw);
}

export async function fetchClaimCaptcha(
  identity: AuthIdentity,
  opts: { signal?: AbortSignal } = {},
): Promise<Blob> {
  const url = `${catalystBase()}/credits/captcha`;
  const res = await signedFetch(identity, url, {
    method: "GET",
    signal: opts.signal,
    metadata: {},
    headers: { accept: "image/png" },
  });
  if (!res.ok) {
    throw new Error(`captcha fetch failed: ${res.status}`);
  }
  return await res.blob();
}

export async function claimCredits(
  identity: AuthIdentity,
  x: number,
  opts: { signal?: AbortSignal } = {},
): Promise<ClaimResult> {
  const raw = await postJSON<unknown>("/credits/captcha", { x }, { identity, ...opts });
  return ClaimCreditsResponseSchema.parse(raw);
}

export type GoalStatus = "progress" | "claim" | "claimed" | "completed";

export type CreditsGoalVM = {
  title: string;
  description: string;
  completed: number;
  total: number;
  reward: number;
  status: GoalStatus;
};

export type CreditsHubVM = {
  hasStartedProgram: boolean;
  available: number;
  earned: number;
  paid: number;
  claimable: number;
  expiresInSeconds: number;
  isBlockedForClaiming: boolean;
  seasonName: string;
  weekNumber: number;
  weekSecondsRemaining: number;
  goals: CreditsGoalVM[];
};

export function goalStatus(g: Goal): GoalStatus {
  if (g.isClaimed) return "claimed";
  const done = g.progress.completedSteps >= g.progress.totalSteps;
  return done ? "claim" : "progress";
}

export function toHubVM(
  seasons: SeasonsData,
  progress: CreditsProgress,
): CreditsHubVM {
  const goals: CreditsGoalVM[] = progress.goals.map((g) => ({
    title: g.title,
    description: g.description,
    completed: g.progress.completedSteps,
    total: Math.max(g.progress.totalSteps, 1),
    reward: g.reward,
    status: goalStatus(g),
  }));

  const claimable = goals
    .filter((g) => g.status === "claim")
    .reduce((sum, g) => sum + g.reward, 0);

  return {
    hasStartedProgram: progress.user.hasStartedProgram,
    available: progress.credits.available,
    earned: progress.credits.earned,
    paid: progress.credits.paid,
    claimable,
    expiresInSeconds: progress.credits.expiresIn,
    isBlockedForClaiming: progress.credits.isBlockedForClaiming,
    seasonName: seasons.currentSeason.season.name,
    weekNumber: seasons.currentSeason.week.weekNumber,
    weekSecondsRemaining: seasons.currentSeason.week.secondsRemaining,
    goals,
  };
}

export function seasonsToShellVM(seasons: SeasonsData): CreditsHubVM {
  return {
    hasStartedProgram: false,
    available: 0,
    earned: 0,
    paid: 0,
    claimable: 0,
    expiresInSeconds: 0,
    isBlockedForClaiming: false,
    seasonName: seasons.currentSeason.season.name,
    weekNumber: seasons.currentSeason.week.weekNumber,
    weekSecondsRemaining: seasons.currentSeason.week.secondsRemaining,
    goals: [],
  };
}

export function formatDuration(seconds: number): string {
  const s = Math.max(0, Math.floor(seconds));
  const days = Math.floor(s / 86400);
  const hours = Math.floor((s % 86400) / 3600);
  if (days > 0) return `${days}d ${hours}h`;
  const minutes = Math.floor((s % 3600) / 60);
  return `${hours}h ${minutes}m`;
}
