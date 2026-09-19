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

const MIN_LOADING_MS = 2200;
const ANTI_STRAND_MS = 75000;
const LOADING_TIMEOUT_MS = 20000;

function jumpIn() {
  fireEvent.click(screen.getByRole("checkbox"));
  const jump = screen.getByText("Continue as guest");
  fireEvent.click(jump.closest("button") ?? jump);
  const skip = screen.getByText("Skip to Genesis Plaza");
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
  vi.useFakeTimers();
});
afterEach(() => {
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
      expect(screen.getByText("Continue as guest")).toBeInTheDocument();
      act(() => { expect(loginWithIdentity(makeIdentity(Date.now() + 86_400_000))).toBe(true); });
      expect(screen.queryByText("Continue as guest")).toBeNull();
      expect(screen.queryByText("Where do you want to go?")).toBeNull();
      expect(window.dclEngineStart).toHaveBeenCalledTimes(1);
      bridge.pushIdentity({ isGuest: false, name: "Saved avatar" });
      bridge.expectNotSent("SetAvatar");
      bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true, pendingAssets: 0 });
      advance(MIN_LOADING_MS);
      expect(worldShown()).toBe(true);
    } finally {
      delete window.dclEngineReady;
      delete window.dclEngineStart;
    }
  });
  test("Loading{ready,avatarLoaded} releases the gate after the min dwell; the gate holds while the avatar is still loading", () => {
    const { bridge } = renderBoot();
    jumpIn();
    expect(document.querySelector(".boot")).toBeTruthy();
    expect(worldShown()).toBe(false);

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: false });
    advance(MIN_LOADING_MS + 2000);
    expect(worldShown()).toBe(false);

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true });
    advance(100);
    expect(worldShown()).toBe(true);
    expect(document.querySelector(".boot")).toBeNull();
  });

  test("ready with pending assets holds the curtain until they settle, then the min dwell applies", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.pushLoading({ percent: 95, ready: true, avatarLoaded: true, pendingAssets: 4 });
    advance(MIN_LOADING_MS + 5000);
    expect(worldShown()).toBe(false);

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true, pendingAssets: 0 });
    advance(100);
    expect(worldShown()).toBe(true);
  });

  test("an alive-but-never-ready engine holds the curtain past the hard timeout and only the anti-strand bound reveals the world", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.pushLoading({ percent: 10, ready: false, avatarLoaded: false });
    advance(MIN_LOADING_MS + 500);
    expect(worldShown()).toBe(false);
    advance(LOADING_TIMEOUT_MS + 1100);
    expect(worldShown()).toBe(false);
    expect(screen.queryByText(/couldn\u{2019}t start/iu)).toBeNull();
    advance(ANTI_STRAND_MS);
    expect(worldShown()).toBe(true);
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
    expect(screen.getByText("Continue as guest")).toBeInTheDocument();
  });

  test("Continue as guest sends one merged SetAvatar once the engine identity arrives, and a later wallet login re-asserts the chosen name at most once", () => {
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

  test("a persisted identity skips the lobby and sends no SetAvatar", () => {
    localStorage.setItem(
      IDENTITY_STORAGE_KEY,
      JSON.stringify(toStoredIdentity(makeIdentity(Date.now() + 48 * 3_600_000))),
    );
    const { bridge } = renderBoot();
    expect(screen.queryByText("Continue as guest")).toBeNull();
    expect(screen.queryByRole("checkbox")).toBeNull();
    expect(document.querySelector(".boot")).toBeTruthy();

    bridge.pushIdentity({ isGuest: false, name: "Alice" });
    bridge.expectNotSent("SetAvatar");

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true });
    advance(MIN_LOADING_MS + 100);
    expect(worldShown()).toBe(true);
    expect(document.querySelector(".boot")).toBeNull();
  });
});
