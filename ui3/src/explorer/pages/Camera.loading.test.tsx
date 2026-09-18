import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { MemoryRouter } from "react-router";
import { afterEach, expect, test, vi } from "vitest";
import { FakeBridge } from "../../test/fakeBridge";
import { useBridgeState } from "../../overlay/bridge";
import CameraPanel from "../../app/panels/Camera.route";

const request = vi.hoisted(() => vi.fn());
vi.mock("../../data/catalyst/client", async original => ({ ...await original<object>(), signedFetch: request }));

afterEach(() => { delete window.dclBridge; vi.useRealTimers(); });

test("gallery failures can be retried and keyboard capture respects the pending shutter", async () => {
  vi.useFakeTimers();
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  let finish!: (result: { status: number; body: string }) => void;
  request.mockImplementationOnce(() => new Promise(resolve => { finish = resolve; })).mockResolvedValue({ status: 200, body: '{"images":[]}' });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function Host({ open }: { open: boolean }) {
    useBridgeState();
    return <QueryClientProvider client={client}><MemoryRouter>{open && <CameraPanel />}</MemoryRouter></QueryClientProvider>;
  }
  const view = render(<Host open={false} />);
  act(() => bridge.pushIdentity());
  view.rerender(<Host open />);
  expect(screen.getByRole("status")).toHaveTextContent("Loading recent photos");
  await act(async () => finish({ status: 503, body: "unavailable" }));
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn't load recent photos");
  await act(async () => fireEvent.click(screen.getByRole("button", { name: "Retry" })));
  expect(screen.queryByRole("alert")).toBeNull();
  fireEvent.keyDown(window, { key: " ", code: "Space" });
  fireEvent.keyDown(window, { key: " ", code: "Space" });
  expect(bridge.sentOf("CapturePhoto")).toHaveLength(1);
  expect(screen.getByRole("status")).toHaveTextContent("Capturing");
  act(() => vi.advanceTimersByTime(30000));
  expect(screen.getByRole("alert")).toHaveTextContent("The camera did not respond");
  fireEvent.keyDown(window, { key: " ", code: "Space" });
  expect(bridge.sentOf("CapturePhoto")).toHaveLength(2);
});
