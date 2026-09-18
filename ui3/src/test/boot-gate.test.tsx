import { afterEach, beforeEach, describe, test, expect, vi } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";

import {
  IDENTITY_STORAGE_KEY,
  loginWithIdentity,
  signOutEngineAuth,
  toStoredIdentity,
} from "../data/auth/engineLogin";
import type { AuthIdentity } from "../data/auth/identity";
import { renderBoot } from "./harness";
import { lifecycleFixture } from "./lifecycleFixture";

const ANTI_STRAND_MS = 75000;
const LOADING_TIMEOUT_MS = 20000;

function jumpIn() {
  fireEvent.click(screen.getByRole("button", { name: "Play as a guest" }));
  fireEvent.click(screen.getByRole("checkbox"));
  fireEvent.click(screen.getByRole("button", { name: "Let\u2019s go" }));
  const skip = screen.getByText("Enter Genesis Plaza");
  fireEvent.click(skip.closest("button") ?? skip);
}

function makeIdentity(expirationMs: number): AuthIdentity {
  const signer = "0xAbCd000000000000000000000000000000000001";
  const ephemeral = "0x1111111111111111111111111111111111111111";
  const expiration = new Date(expirationMs).toISOString();
  return {
    signer: signer.toLowerCase(),
    ephemeral: {
      address: ephemeral,
      privateKey:
        "0x59c6995e998f97a5a0044966f0945389dc9e86dae88c7a8412f4603b6b78690d",
    },
    expiration,
    authChain: [
      { type: "SIGNER", payload: signer, signature: "" },
      {
        type: "ECDSA_EPHEMERAL",
        payload: [
          "Decentraland Login",
          `Ephemeral address: ${ephemeral}`,
          `Expiration: ${expiration}`,
        ].join("\n"),
        signature: "0xsigsig",
      },
    ],
  };
}

const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));
const worldShown = () => screen.queryByTestId("world-content") !== null;

beforeEach(() => {
  window.dclEngineReady = true;
  window.dclEngineStart = vi.fn();
  window.dclEngineConnectLobby = vi.fn();
  vi.useFakeTimers();
});
afterEach(() => {
  delete window.dclEngineReady;
  delete window.dclEngineStart;
  delete window.dclEngineConnectLobby;
  vi.useRealTimers();
  signOutEngineAuth();
  localStorage.removeItem(IDENTITY_STORAGE_KEY);
});

describe("boot gate release", () => {
  test("wallet sign-in enters without guest confirmation or overwriting the saved avatar", () => {
    window.dclEngineReady = true;
    window.dclEngineStart = vi.fn();
    try {
      const { bridge } = renderBoot();
      expect(screen.getByText("Play as a guest")).toBeInTheDocument();
      act(() => { expect(loginWithIdentity(makeIdentity(Date.now() + 86_400_000))).toBe(true); });
      expect(screen.queryByText("Play as a guest")).toBeNull();
      expect(screen.queryByText("Where do you want to go?")).toBeNull();
      expect(window.dclEngineStart).not.toHaveBeenCalled();
      fireEvent.click(screen.getByText("Enter Genesis Plaza"));
      expect(window.dclEngineStart).toHaveBeenCalledTimes(1);
      bridge.pushIdentity({ isGuest: false, name: "Saved avatar" });
      bridge.expectNotSent("SetAvatar");
      bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true, pendingAssets: 0 });
      expect(worldShown()).toBe(true);
    } finally {
      delete window.dclEngineReady;
      delete window.dclEngineStart;
    }
  });
  test("Loading{ready,avatarLoaded} releases the gate; the gate holds while the avatar is still loading", () => {
    const { bridge } = renderBoot();
    jumpIn();
    expect(worldShown()).toBe(false);

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: false });
    advance(5000);
    expect(worldShown()).toBe(false);

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true });
    advance(100);
    expect(worldShown()).toBe(true);
  });

  test("ready with pending assets holds the curtain until they settle", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.pushLoading({ percent: 95, ready: true, avatarLoaded: true, pendingAssets: 4 });
    advance(5000);
    expect(worldShown()).toBe(false);

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true, pendingAssets: 0 });
    advance(100);
    expect(worldShown()).toBe(true);
  });

  test("an alive-but-never-ready engine times out without advertising world arrival", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.pushLoading({ percent: 10, ready: false, avatarLoaded: false });
    advance(5000);
    expect(worldShown()).toBe(false);
    advance(LOADING_TIMEOUT_MS + 1100);
    expect(worldShown()).toBe(false);
    expect(screen.queryByText(/couldn\u{2019}t start/iu)).toBeNull();
    advance(ANTI_STRAND_MS);
    expect(worldShown()).toBe(false);
    expect(screen.getByText(/couldn\u{2019}t start/iu)).toBeInTheDocument();
  });

  test("committed lifecycle readiness takes precedence over legacy loading signals", () => {
    const { bridge } = renderBoot();
    jumpIn();
    const waiting = lifecycleFixture();
    waiting.readiness.canExplore = false;
    bridge.push({ kind: "lifecycle", snapshot: waiting });
    bridge.pushLoading({ ready: true, avatarLoaded: true, pendingAssets: 0 });
    advance(5000);
    expect(worldShown()).toBe(false);
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({ revision: 2 }) });
    advance(100);
    expect(worldShown()).toBe(true);
  });

  test("startup already completed before Jump In stays ready without another engine transition", () => {
    const { bridge } = renderBoot();
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture() });
    jumpIn();
    expect(worldShown()).toBe(true);
  });

  test("only engine-confirmed degraded placement permits entry with unsettled assets", () => {
    const { bridge } = renderBoot();
    jumpIn();
    const snapshot = lifecycleFixture();
    snapshot.readiness = { ...snapshot.readiness, placement: "degraded", pendingAssets: 3, avatarReady: false };
    bridge.push({ kind: "lifecycle", snapshot });
    expect(worldShown()).toBe(true);
  });

  test("realm failure is immediately recoverable instead of eventually revealing an uninitialized world", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.push({ kind: "lifecycle", snapshot: lifecycleFixture({
      realm: { phase: "failed", generation: 1, destination: "private.dcl.eth", error: "Access denied" },
    }) });
    expect(worldShown()).toBe(false);
    expect(screen.getByText(/couldn\u{2019}t start/iu)).toBeInTheDocument();
  });

  test("wasm progress and scene progress each drive their half of the bar", () => {
    const { bridge } = renderBoot();
    act(() => {
      window.dispatchEvent(new CustomEvent("dcl-loading", { detail: { percent: 80 } }));
    });
    jumpIn();
    expect(screen.getByText(/40%/)).toBeInTheDocument();
    bridge.pushLoading({ percent: 60, ready: false, avatarLoaded: false });
    expect(screen.getByText(/80%/)).toBeInTheDocument();
  });

  test("with no bridge signal at all the hard timeout shows the recoverable stalled screen", () => {
    renderBoot();
    jumpIn();
    advance(LOADING_TIMEOUT_MS - 1000);
    expect(worldShown()).toBe(false);

    advance(1100);
    expect(worldShown()).toBe(false);
    expect(screen.getByText(/couldn\u{2019}t start/iu)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /back to lobby/i }));
    expect(screen.getByText("Enter Genesis Plaza")).toBeInTheDocument();
  });

  test("Playing as a guest sends one merged SetAvatar once the engine identity arrives, and a later wallet login re-asserts the chosen name at most once", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.expectNotSent("SetAvatar");

    bridge.pushIdentity({ isGuest: true, name: "Bevy_User" });
    const payload = bridge.expectSent("SetAvatar");
    expect(payload.base?.bodyShapeUrn).toMatch(/base-avatars/);
    const chosen = payload.base?.name;
    expect(chosen).toBeTruthy();
    expect(chosen).not.toBe("Bevy_User");
    expect(bridge.sentOf("SetAvatar")).toHaveLength(1);

    bridge.pushIdentity({ isGuest: false, name: "Bevy_User" });
    const sends = bridge.sentOf("SetAvatar");
    expect(sends).toHaveLength(2);
    expect(sends[1]?.base?.name).toBe(chosen);

    bridge.pushIdentity({ isGuest: false, name: chosen });
    expect(bridge.sentOf("SetAvatar")).toHaveLength(2);
  });

  test("a persisted identity connects in the lobby and sends no SetAvatar", () => {
    localStorage.setItem(
      IDENTITY_STORAGE_KEY,
      JSON.stringify(toStoredIdentity(makeIdentity(Date.now() + 48 * 3_600_000))),
    );
    const { bridge } = renderBoot();
    expect(screen.queryByText("Play as a guest")).toBeNull();
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(screen.getByText("Enter Genesis Plaza")).toBeInTheDocument();
    expect(window.dclEngineStart).not.toHaveBeenCalled();
    fireEvent.click(screen.getByText("Enter Genesis Plaza"));

    bridge.pushIdentity({ isGuest: false, name: "Alice" });
    bridge.expectNotSent("SetAvatar");

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true });
    expect(worldShown()).toBe(true);
  });
});
