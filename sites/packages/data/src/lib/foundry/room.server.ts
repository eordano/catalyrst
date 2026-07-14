import { createHash } from "node:crypto";

import { AccessToken, RoomServiceClient } from "livekit-server-sdk";

import { FoundryStateError } from "./db.server";
import { getPersona } from "./persona.server";

// Every foundry page is a LiveKit room named by its path, on the same SFU the
// comms stack already runs (deploy-livekit). The server mints the join token;
// the browser only ever holds a room-scoped JWT. Identity is a hash of the sid
// — long enough to be unique, never the sid itself — and the display name is
// the persona the visitor authored, or the honest visitor badge.

const PATH_RE = /^\/foundry(\/(?!\.+(?:\/|$))[A-Za-z0-9._~-]+)*\/?$/;

export function roomIdentity(sid: string): string {
  return createHash("sha256").update(sid).digest("hex").slice(0, 16);
}

/** `foundry:/foundry/gdd/flagrush-v1` — one room per page, query stripped. */
export function roomNameForPath(path: string): string {
  const trimmed = path.replace(/\/+$/, "");
  if (!PATH_RE.test(trimmed)) {
    throw new FoundryStateError("Rooms exist only for foundry pages.");
  }
  return `foundry:${trimmed}`;
}

function env(name: string): string | null {
  return process.env[name]?.trim() || null;
}

export interface RoomTicket {
  token: string;
  wsUrl: string;
  room: string;
  identity: string;
  name: string;
}

/** Null when this deployment has no LiveKit configured — the dock then renders
 *  nothing rather than a dead widget. */
export function roomsConfigured(): boolean {
  return Boolean(
    env("LIVEKIT_WS_URL") && env("LIVEKIT_API_KEY") && env("LIVEKIT_API_SECRET"),
  );
}

export async function mintRoomTicket(input: {
  path: string;
  sid: string;
}): Promise<RoomTicket> {
  const wsUrl = env("LIVEKIT_WS_URL");
  const apiKey = env("LIVEKIT_API_KEY");
  const apiSecret = env("LIVEKIT_API_SECRET");
  if (!wsUrl || !apiKey || !apiSecret) {
    throw new FoundryStateError("This deployment has no room server configured.");
  }

  const room = roomNameForPath(input.path);
  const identity = roomIdentity(input.sid);
  const persona = await getPersona(input.sid);
  const name = persona?.displayName ?? `visitor ${identity.slice(0, 4)}`;

  const at = new AccessToken(apiKey, apiSecret, {
    identity,
    name,
    ttl: "2h",
  });
  at.addGrant({
    room,
    roomJoin: true,
    canPublish: true,
    canPublishData: true,
    canSubscribe: true,
  });

  return { token: await at.toJwt(), wsUrl, room, identity, name };
}

export interface RoomPresence {
  /** The page path the room is named after. */
  path: string;
  count: number;
}

/** Who is connected right now, per page room — a live SFU reading, not a DB
 *  row, so it carries its own failure mode: null when the server is
 *  unconfigured OR the probe fails/times out, and the surface says it could
 *  not read rather than rendering an invented zero. */
export async function listRoomPresence(): Promise<RoomPresence[] | null> {
  const wsUrl = env("LIVEKIT_WS_URL");
  const apiKey = env("LIVEKIT_API_KEY");
  const apiSecret = env("LIVEKIT_API_SECRET");
  if (!wsUrl || !apiKey || !apiSecret) return null;
  try {
    // The public edge only forwards the websocket path; the admin twirp API is
    // reached on the SFU's local HTTP port, named explicitly by env.
    const httpUrl = env("LIVEKIT_HTTP_URL") ?? wsUrl.replace(/^ws/, "http");
    const svc = new RoomServiceClient(httpUrl, apiKey, apiSecret);
    const rooms = await Promise.race([
      svc.listRooms(),
      new Promise<never>((_, reject) =>
        setTimeout(() => reject(new Error("presence probe timed out")), 2000),
      ),
    ]);
    return rooms
      .filter((r) => r.name.startsWith("foundry:") && r.numParticipants > 0)
      .map((r) => ({
        path: r.name.slice("foundry:".length),
        count: r.numParticipants,
      }));
  } catch {
    return null;
  }
}
