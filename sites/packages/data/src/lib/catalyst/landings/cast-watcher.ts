import { z } from "zod";

export const WatcherCredentialsSchema = z.object({
  url: z.string().min(1),
  token: z.string().min(1),
  roomId: z.string().min(1),
  identity: z.string().min(1),
  placeName: z.string().optional(),
});
export type WatcherCredentials = z.infer<typeof WatcherCredentialsSchema>;

export const StreamInfoSchema = z.object({
  placeName: z.string().min(1),
  placeId: z.string().optional(),
  location: z.string().min(1),
  isWorld: z.boolean(),
});
export type StreamInfo = z.infer<typeof StreamInfoSchema>;

export const WATCH_ERRORS = ["no_active_stream", "access_expired"] as const;
export type WatchError = (typeof WATCH_ERRORS)[number];

export type WatchStatus = "live" | "waiting" | "expired";

export type WatchResult = {
  status: WatchStatus;
  location: string;
  isWorld: boolean;
  placeName: string;
  identity: string;
  credentials?: WatcherCredentials;
  error?: WatchError;
  fallback?: boolean;
};

export function isWorldLocation(location: string): boolean {
  return !/^-?\d+\s*,\s*-?\d+$/.test(location.trim());
}

