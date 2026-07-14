import path from "node:path";

import { data } from "react-router";

import {
  ensureSid,
  readVerifiedWallet,
  resolveAssignment,
  serializeSidCookie,
  type Assignment,
} from "./assign";
import { parseStory } from "./context";
import { trackExposure } from "../telemetry/track";

export function parseVariantOverride(
  url: URL,
  experimentKey: string,
): string | undefined {
  const raw = url.searchParams.get("variant");
  if (!raw) return undefined;
  const sep = raw.indexOf(":");
  if (sep === -1) return undefined;
  const key = raw.slice(0, sep);
  const variant = raw.slice(sep + 1);
  if (key !== experimentKey || !variant) return undefined;
  return variant;
}

export function sidLoader(request: Request) {
  const { sid, created } = ensureSid(request);
  const wallet = readVerifiedWallet(request);
  const headers = created
    ? { "Set-Cookie": serializeSidCookie(sid) }
    : undefined;
  return {
    sid,
    wallet,
    userKey: wallet ?? sid,
    created,
    wrap: <T,>(payload: T, init?: { status?: number }) =>
      data(payload, { status: init?.status, headers }),
  };
}

export async function storyLoader(
  request: Request,
  storyDir: string,
  fallback: Assignment,
  options?: { skipExposure?: boolean },
) {
  const base = sidLoader(request);
  let assignment = fallback;
  try {
    const story = parseStory(
      path.join(process.cwd(), "packages", "features", "src", "stories", storyDir),
    );
    assignment = await resolveAssignment(base.sid, story, { user: base.userKey });
    const override = parseVariantOverride(
      new URL(request.url),
      story.experiment.key,
    );
    if (override) {
      const v = story.experiment.variants.find((x) => x.id === override);
      if (v) {
        assignment = {
          variant: v.id,
          flags: v.flags,
          experimentKey: story.experiment.key,
        };
      }
    }
  } catch {
  }
  if (!options?.skipExposure) {
    trackExposure({
      sid: base.sid,
      story: storyDir,
      variant: assignment.variant,
      experimentKey: assignment.experimentKey,
    });
  }
  return { ...base, assignment };
}
