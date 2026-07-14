import { getJSON, type GetOptions } from "../client";
import {
  WatcherCredentialsSchema,
  StreamInfoSchema,
  isWorldLocation,
  type WatchResult,
  type WatcherCredentials,
  type StreamInfo,
} from "./cast-watcher";

export type WatchIntent = "live" | "waiting" | "expired";

export function fallbackPlaceName(location: string): string {
  const loc = location.trim();
  if (!loc) return "this Decentraland Cast";
  return isWorldLocation(loc) ? loc : `Parcel ${loc}`;
}

async function fetchStreamInfo(
  location: string,
  opts: GetOptions,
): Promise<StreamInfo | null> {
  try {
    const raw = await getJSON<unknown>(
      `/cast/stream-info/${encodeURIComponent(location)}`,
      opts,
    );
    return StreamInfoSchema.parse(raw);
  } catch {
    return null;
  }
}

async function fetchWatcherCredentials(
  location: string,
  identity: string,
  opts: GetOptions,
): Promise<WatcherCredentials | null> {
  try {
    const raw = await getJSON<unknown>(
      `/cast/watcher-token/${encodeURIComponent(location)}`,
      opts,
    );
    const parsed = WatcherCredentialsSchema.parse(raw);
    return parsed.identity ? parsed : { ...parsed, identity };
  } catch {
    return null;
  }
}

export async function resolveWatch(
  location: string,
  identity: string,
  intent: WatchIntent,
  opts: GetOptions = {},
): Promise<WatchResult> {
  const info = await fetchStreamInfo(location, opts);
  const infoFallback = info === null;
  const isWorld = info?.isWorld ?? isWorldLocation(location);
  const placeName = info?.placeName || fallbackPlaceName(location);

  if (intent === "expired") {
    return {
      status: "expired",
      location,
      isWorld,
      placeName,
      identity,
      error: "access_expired",
      fallback: infoFallback,
    };
  }
  if (intent === "waiting") {
    return {
      status: "waiting",
      location,
      isWorld,
      placeName,
      identity,
      error: "no_active_stream",
      fallback: infoFallback,
    };
  }

  const credentials = await fetchWatcherCredentials(location, identity, opts);
  if (!credentials) {
    return {
      status: "waiting",
      location,
      isWorld,
      placeName,
      identity,
      error: "no_active_stream",
      fallback: true,
    };
  }

  return {
    status: "live",
    location,
    isWorld,
    placeName: credentials.placeName || placeName,
    identity: credentials.identity || identity,
    credentials,
    fallback: false,
  };
}
