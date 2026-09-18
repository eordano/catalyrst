import { expect, test } from "vitest";
import { canRestoreSceneSession, createSceneSession, reduceSceneSession } from "./scene-session";

test("a session accepts one handshake and rejects stale hydration completion", () => {
  const first = createSceneSession(1);
  const hydration = first.beginHydration();
  expect(hydration).not.toBeNull();
  expect(first.beginHydration()).toBeNull();
  first.retire();
  expect(first.complete(hydration!)).toBe(false);
  expect(first.phase()).toBe("retired");

  const next = createSceneSession(2);
  const nextHydration = next.beginHydration();
  expect(next.complete(nextHydration!)).toBe(true);
  expect(next.phase()).toBe("ready");
});

test("a hydration timeout reaches error and cannot later restore or become ready", () => {
  const session = createSceneSession(3);
  const hydration = session.beginHydration();
  expect(canRestoreSceneSession(session, hydration!, false)).toBe(false);
  expect(session.timeoutHydration(hydration!)).toBe(true);
  expect(session.isHydrating(hydration!)).toBe(false);
  expect(canRestoreSceneSession(session, hydration!, true)).toBe(false);
  expect(session.phase()).toBe("error");
  expect(session.complete(hydration!)).toBe(false);
});

test("a handshake timeout reaches error", () => {
  const session = createSceneSession(4);
  expect(session.timeoutHandshake()).toBe(true);
  expect(session.phase()).toBe("error");
});

test("a scene without hydration becomes ready once", () => {
  const session = createSceneSession(5);
  expect(session.readyWithoutHydration()).toBe(true);
  expect(session.readyWithoutHydration()).toBe(false);
  expect(session.phase()).toBe("ready");
});

test("the transition table drives the reducer", () => {
  expect(reduceSceneSession("handshake", "beginHydration")).toBe("hydrating");
  expect(reduceSceneSession("ready", "complete")).toBe("ready");
});
