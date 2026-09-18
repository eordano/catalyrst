import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import { VoiceControls } from "./VoiceChat";
import { FakeBridge } from "../../test/fakeBridge";

const originalMediaDevices = Object.getOwnPropertyDescriptor(navigator, "mediaDevices");
afterEach(() => {
  delete window.dclBridge;
  if (originalMediaDevices) Object.defineProperty(navigator, "mediaDevices", originalMediaDevices);
  else Reflect.deleteProperty(navigator, "mediaDevices");
  vi.restoreAllMocks();
});

test("requests permission, releases the preflight stream and waits for engine acknowledgement", async () => {
  const stop = vi.fn();
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia: vi.fn().mockResolvedValue({ getTracks: () => [{ stop }] }) } });
  const bridge = new FakeBridge(); window.dclBridge = bridge;
  render(<VoiceControls />);
  act(() => { bridge.pushMic({ available: true, enabled: false }); });
  fireEvent.click(screen.getByRole("button", { name: "Speak" }));
  await waitFor(() => expect(stop).toHaveBeenCalledOnce());
  expect(bridge.expectSent("SetMic")).toEqual({ enabled: true });
  expect(screen.getByRole("button", { name: "Updating microphone\u{2026}" })).toBeDisabled();
  act(() => { bridge.pushMic({ available: true, enabled: true }); });
  expect(screen.getByRole("button", { name: /Mic on/ })).toBeEnabled();
});

test("a denied permission stays muted and displays the failure", async () => {
  Object.defineProperty(navigator, "mediaDevices", { configurable: true, value: { getUserMedia: vi.fn().mockRejectedValue(new Error("Permission denied")) } });
  const bridge = new FakeBridge(); window.dclBridge = bridge;
  render(<VoiceControls />);
  act(() => { bridge.pushMic({ available: true, enabled: false }); });
  fireEvent.click(screen.getByRole("button", { name: "Speak" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Permission denied");
  expect(bridge.sent.some((c) => c.action === "SetMic")).toBe(false);
  expect(screen.getByRole("switch", { name: "Microphone" })).toHaveAttribute("aria-checked", "false");
});
