import { act, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import { FakeBridge } from "../../test/fakeBridge";
import { useBridgeState } from "../../overlay/bridge";
import Camera from "./Camera";

const request = vi.hoisted(() => vi.fn());
vi.mock("../../data/catalyst/client", async original => ({ ...await original<object>(), signedFetch: request }));
afterEach(() => { delete window.dclBridge; });

test("lobby screenshots and replayed photos are never uploaded when opening Camera", async () => {
  const bridge = new FakeBridge();
  const subscribe = bridge.onState;
  bridge.onState = callback => {
    const unsubscribe = subscribe(callback);
    callback({ kind: "photo", dataUrl: "data:image/png;base64,bG9iYnk=" });
    return unsubscribe;
  };
  window.dclBridge = bridge;
  request.mockResolvedValue({ status: 200, body: '{"images":[]}' });
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  function Host({ open }: { open: boolean }) {
    useBridgeState();
    return <QueryClientProvider client={client}>{open && <Camera />}</QueryClientProvider>;
  }
  const view = render(<Host open={false} />);
  act(() => bridge.pushIdentity());
  await act(async () => view.rerender(<Host open />));
  act(() => bridge.push({ kind: "photo", dataUrl: "data:image/png;base64,bG9iYnk=" }));
  expect(request.mock.calls.some(([, opts]) => opts.method === "POST")).toBe(false);
  fireEvent.click(screen.getByRole("button", { name: /take photo/i }));
  await act(async () => bridge.push({ kind: "photo", dataUrl: "data:image/png;base64,cGhvdG8=" }));
  expect(request.mock.calls.filter(([, opts]) => opts.method === "POST")).toHaveLength(1);
});
