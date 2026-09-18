import { useContext } from "react";
import { WorldEntryContext } from "./WorldEntry";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { LifecycleSnapshot } from "../generated/bridge/LifecycleSnapshot";
import BootGate from "./BootGate";

const bridge = vi.hoisted(() => ({ listener: null as null | ((value: LifecycleSnapshot) => void) }));
vi.mock("../overlay/bridge", () => ({
  subscribeLifecycle: (listener: (value: LifecycleSnapshot) => void) => {
    bridge.listener = listener;
    return () => { bridge.listener = null; };
  },
}));
vi.mock("../data/auth/engineLogin", () => ({
  IDENTITY_STORAGE_KEY: "boot-test-identity",
  initEngineAuth: vi.fn(),
  shouldAutoJumpIn: () => true,
  getEngineAuthState: () => ({ address: null }),
  subscribeEngineAuth: () => () => {},
}));
vi.mock("../explorer/workflows/LobbyNew", () => ({ default: () => <p>Lobby</p> }));
vi.mock("../explorer/workflows/Loading", () => ({ default: () => <p>Loading world</p> }));
vi.mock("../explorer/components/FpsMeter", () => ({ default: () => null }));

function readiness(overrides: Partial<LifecycleSnapshot["readiness"]> = {}, failed = false) {
  act(() => bridge.listener!({
    readiness: { canExplore: true, pendingAssets: 0, avatarReady: true, placement: "ready", ...overrides },
    realm: { phase: failed ? "failed" : "ready", error: failed ? "Connection failed" : null },
    travel: null,
  } as unknown as LifecycleSnapshot));
}

beforeEach(() => {
  vi.useFakeTimers();
  window.history.replaceState({}, "", "/");
  window.dclBridge = { onState: () => () => {}, send: vi.fn() } as unknown as typeof window.dclBridge;
  window.dclEngineReady = true;
  window.dclEngineStart = vi.fn();
  window.dclEngineConnectLobby = vi.fn();
});
afterEach(() => {
  cleanup();
  window.history.replaceState({}, "", "/");
  delete window.dclBridge;
  delete window.dclEngineStart;
  delete window.dclEngineConnectLobby;
  delete window.dclEngineReady;
  vi.useRealTimers();
});

describe("world startup readiness", () => {
  it("returns a signed-in user to the lobby without loading a scene", () => {
    window.history.replaceState({}, "", "/");
    render(<BootGate><p>Lobby controls</p></BootGate>);
    expect(screen.getByText("Lobby controls")).toBeInTheDocument();
    expect(document.documentElement.dataset.dclBootPhase).toBe("lobby");
    expect(window.dclEngineStart).not.toHaveBeenCalled();
  });

  it.each(["/?position=0,0", "/?realm=test.eth&position=-2,3"])("enters a valid position link without visiting the lobby: %s", search => {
    window.history.replaceState({}, "", search);
    render(<BootGate><Controls /></BootGate>);
    expect(screen.getByText("Loading world")).toBeInTheDocument();
    expect(window.dclEngineStart).toHaveBeenCalledOnce();
    expect(window.dclEngineConnectLobby).not.toHaveBeenCalled();
    readiness();
    expect(screen.getByText("World controls")).toBeInTheDocument();
  });

  it.each(["/?position=bad", "/?realm=test.eth"])("keeps links without valid coordinates in the lobby: %s", search => {
    window.history.replaceState({}, "", search);
    render(<BootGate><Controls /></BootGate>);
    expect(screen.getByText("Enter world")).toBeInTheDocument();
    expect(window.dclEngineStart).not.toHaveBeenCalled();
  });

  it("reveals a ready world without an artificial minimum loading time", () => {
    render(<BootGate><Controls /></BootGate>);
    fireEvent.click(screen.getByText("Enter world"));
    expect(screen.getByText("Loading world")).toBeInTheDocument();
    readiness();
    expect(screen.getByText("World controls")).toBeInTheDocument();
    expect(screen.queryByText("Loading world")).not.toBeInTheDocument();
  });

  it("waits for engine, assets and avatar readiness", () => {
    render(<BootGate><Controls /></BootGate>);
    fireEvent.click(screen.getByText("Enter world"));
    readiness({ canExplore: false });
    expect(screen.queryByText("World controls")).not.toBeInTheDocument();
    readiness({ pendingAssets: 1 });
    expect(screen.queryByText("World controls")).not.toBeInTheDocument();
    readiness({ avatarReady: false });
    expect(screen.queryByText("World controls")).not.toBeInTheDocument();
    readiness();
    expect(screen.getByText("World controls")).toBeInTheDocument();
  });

  it("retains the startup timeout and keeps failed realms out of the world", () => {
    render(<BootGate><Controls /></BootGate>);
    fireEvent.click(screen.getByText("Enter world"));
    act(() => { vi.advanceTimersByTime(10 * 60_000); });
    expect(screen.getByRole("alert")).toHaveTextContent("The explorer did not report readiness in time.");
    readiness({}, true);
    expect(screen.queryByText("World controls")).not.toBeInTheDocument();
    readiness();
    expect(screen.queryByText("World controls")).not.toBeInTheDocument();
  });

  it.each([false, true])("recovers a slow engine when it actually becomes ready (degraded: %s)", degraded => {
    render(<BootGate><Controls /></BootGate>);
    fireEvent.click(screen.getByText("Enter world"));
    readiness({ canExplore: false, pendingAssets: 19 });
    act(() => { vi.advanceTimersByTime(10 * 60_000); });
    expect(screen.getByRole("alert")).toHaveTextContent("The explorer did not report readiness in time.");
    readiness({ canExplore: false, pendingAssets: 19 });
    expect(screen.queryByText("World controls")).not.toBeInTheDocument();
    readiness(degraded ? { placement: "degraded", pendingAssets: 19 } : {});
    expect(screen.getByText("World controls")).toBeInTheDocument();
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
  });
});

function Controls() {
  const entry = useContext(WorldEntryContext);
  return entry?.pending ? <button onClick={() => entry.enter(null)}>Enter world</button> : <p>World controls</p>;
}
