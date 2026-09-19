import { flagState } from "./featureFlags";

export const LOBBY_FLAG = "2026-09-lobby";
export const LOBBY_STORAGE = `dcl.feature.${LOBBY_FLAG}`;

export function lobbyEnabled(): boolean { return flagState(LOBBY_FLAG).enabled; }

export function lobbyIsInitialView(): boolean {
  return lobbyEnabled();
}
