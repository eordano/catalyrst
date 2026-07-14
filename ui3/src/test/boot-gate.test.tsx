import { afterEach, beforeEach, describe, test, expect, vi } from "vitest";
import { act, fireEvent, screen } from "@testing-library/react";

import {
  IDENTITY_STORAGE_KEY,
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
  // Continue as guest now lands on the destination picker; skipping it starts the loading gate.
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

beforeEach(() => {
  vi.useFakeTimers();
});
afterEach(() => {
  vi.useRealTimers();
  signOutEngineAuth();
  localStorage.removeItem(IDENTITY_STORAGE_KEY);
});

describe("boot gate release", () => {
  test("Loading{ready,avatarLoaded} push releases the gate after the min dwell", () => {
    const { bridge } = renderBoot();
    jumpIn();
    expect(document.querySelector(".boot")).toBeTruthy();
    expect(screen.queryByTestId("world-content")).toBeNull();

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true });
    advance(MIN_LOADING_MS - 200);
    expect(screen.queryByTestId("world-content")).toBeNull();

    advance(300);
    expect(screen.getByTestId("world-content")).toBeInTheDocument();
    expect(document.querySelector(".boot")).toBeNull();
  });

  test("gate holds while avatarLoaded=false, releases once the avatar lands", () => {
    const { bridge } = renderBoot();
    jumpIn();

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: false });
    advance(MIN_LOADING_MS + 2000);
    expect(screen.queryByTestId("world-content")).toBeNull();

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true });
    advance(100);
    expect(screen.getByTestId("world-content")).toBeInTheDocument();
  });

  test("an alive-but-not-ready engine holds the curtain until the anti-strand bound", () => {
    const { bridge } = renderBoot();
    jumpIn();

    bridge.pushLoading({ percent: 40, ready: false, avatarLoaded: true });
    advance(MIN_LOADING_MS + 500);
    expect(screen.queryByTestId("world-content")).toBeNull();

    // No grace reveal: an alive engine says nothing about the scene's assets.
    advance(30000);
    expect(screen.queryByTestId("world-content")).toBeNull();

    advance(ANTI_STRAND_MS);
    expect(screen.getByTestId("world-content")).toBeInTheDocument();
  });

  test("ready with pending assets holds the curtain until they settle", () => {
    const { bridge } = renderBoot();
    jumpIn();

    bridge.pushLoading({ percent: 95, ready: true, avatarLoaded: true, pendingAssets: 4 });
    advance(MIN_LOADING_MS + 5000);
    expect(screen.queryByTestId("world-content")).toBeNull();

    bridge.pushLoading({ percent: 100, ready: true, avatarLoaded: true, pendingAssets: 0 });
    advance(100);
    expect(screen.getByTestId("world-content")).toBeInTheDocument();
  });

  test("dcl-loading window events drive the wasm progress display", () => {
    renderBoot();
    act(() => {
      window.dispatchEvent(
        new CustomEvent("dcl-loading", { detail: { percent: 80 } }),
      );
    });
    jumpIn();
    expect(screen.getByText(/40%/)).toBeInTheDocument();
  });

  test("scene progress pushes take over the top half of the progress bar", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.pushLoading({ percent: 60, ready: false, avatarLoaded: false });
    expect(screen.getByText(/80%/)).toBeInTheDocument();
  });

  test("with no bridge signal at all the hard timeout shows the recoverable stalled screen", () => {
    renderBoot();
    jumpIn();
    advance(LOADING_TIMEOUT_MS - 1000);
    expect(screen.queryByTestId("world-content")).toBeNull();

    advance(1100);
    expect(screen.queryByTestId("world-content")).toBeNull();
    expect(screen.getByText(/couldn\u{2019}t start/iu)).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: /back to lobby/i }));
    expect(screen.getByText("Continue as guest")).toBeInTheDocument();
  });

  test("engine alive but never ready: the anti-strand bound still reveals the world", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.pushLoading({ percent: 10, ready: false, avatarLoaded: false });
    advance(LOADING_TIMEOUT_MS + 1100);
    expect(screen.queryByTestId("world-content")).toBeNull();
    advance(ANTI_STRAND_MS - LOADING_TIMEOUT_MS);
    expect(screen.getByTestId("world-content")).toBeInTheDocument();
  });

  test("Continue as guest sends one merged SetAvatar once the engine identity arrives", () => {
    const { bridge } = renderBoot();
    jumpIn();
    bridge.expectNotSent("SetAvatar");

    bridge.pushIdentity({ isGuest: true, name: "guest" });
    const payload = bridge.expectSent("SetAvatar");
    expect(payload.base?.bodyShapeUrn).toMatch(/base-avatars/);
    expect(payload.base?.name).toBeTruthy();
    expect(bridge.sentOf("SetAvatar")).toHaveLength(1);
  });

  test("persisted identity skips the lobby and sends no SetAvatar", () => {
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
    expect(screen.getByTestId("world-content")).toBeInTheDocument();
    expect(document.querySelector(".boot")).toBeNull();
  });

  test("a wallet login completing after Continue as guest does not strand the engine on its default name", () => {
    const { bridge } = renderBoot();
    jumpIn();

    bridge.pushIdentity({ isGuest: true, name: "Bevy_User" });
    const chosen = bridge.expectSent("SetAvatar").base?.name;
    expect(chosen).toBeTruthy();
    expect(chosen).not.toBe("Bevy_User");

    // The engine's login task replaces the profile wholesale when it completes.
    // With no profile deployed for the address the replacement is the default,
    // so the engine reports the chosen name gone.
    bridge.pushIdentity({ isGuest: false, name: "Bevy_User" });

    const sends = bridge.sentOf("SetAvatar");
    expect(sends).toHaveLength(2);
    expect(sends[1]?.base?.name).toBe(chosen);
  });

  test("the chosen name is re-asserted at most once per engine regression", () => {
    const { bridge } = renderBoot();
    jumpIn();

    bridge.pushIdentity({ isGuest: true, name: "Bevy_User" });
    const chosen = bridge.expectSent("SetAvatar").base?.name;

    bridge.pushIdentity({ isGuest: false, name: chosen });
    expect(bridge.sentOf("SetAvatar")).toHaveLength(1);
  });
});
