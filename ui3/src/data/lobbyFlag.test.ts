import { afterEach, expect, test, vi } from "vitest";
import { lobbyEnabled, lobbyIsInitialView, LOBBY_FLAG, LOBBY_STORAGE } from "./lobbyFlag";

afterEach(() => { localStorage.removeItem(LOBBY_STORAGE); history.replaceState(null, "", "/"); vi.restoreAllMocks(); });

test("lobby is enabled by default without a special link or stored preference", () => {
  expect(lobbyEnabled()).toBe(true);
  expect(lobbyIsInitialView()).toBe(true);
  history.replaceState(null, "", `/?${LOBBY_FLAG}=1`);
  expect(lobbyIsInitialView()).toBe(true);
  expect(localStorage.getItem(LOBBY_STORAGE)).toBeNull();
});

test("URL opt-out wins over saved opt-in", () => {
  localStorage.setItem(LOBBY_STORAGE, "1");
  expect(lobbyEnabled()).toBe(true);
  history.replaceState(null, "", `/?${LOBBY_FLAG}=0`);
  expect(lobbyIsInitialView()).toBe(false);
});

test.each(["position=10,-20", "realm=example.dcl.eth"])("destination links keep the lobby visible: %s", (destination) => {
  history.replaceState(null, "", `/?${destination}`);
  expect(lobbyEnabled()).toBe(true);
  expect(lobbyIsInitialView()).toBe(true);
});

test("storage denial still opens the default lobby", () => {
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => { throw new Error("blocked"); });
  expect(lobbyEnabled()).toBe(true);
});

test("a saved opt-out disables the lobby and an explicit URL opt-in overrides it", () => {
  localStorage.setItem(LOBBY_STORAGE, "0");
  expect(lobbyEnabled()).toBe(false);
  history.replaceState(null, "", `/?${LOBBY_FLAG}=1`);
  expect(lobbyEnabled()).toBe(true);
});

test("blank and malformed destination parameters do not suppress the default lobby", () => {
  history.replaceState(null, "", `/?${LOBBY_FLAG}=1&realm=&position=nope`);
  expect(lobbyIsInitialView()).toBe(true);
});
