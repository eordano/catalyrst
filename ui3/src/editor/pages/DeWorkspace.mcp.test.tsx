import { cleanup, render, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, expect, test, vi } from "vitest";

import DeWorkspace from "./DeWorkspace";
import { autoConnect } from "../mcp-bridge";

vi.mock("../mcp-bridge", () => ({ autoConnect: vi.fn() }));

class Channel {
  onmessage: ((event: { data: unknown }) => void) | null = null;
  postMessage(): void {}
  close(): void {}
}

const firstDispose = vi.fn();
const secondDispose = vi.fn();
const first = "/_play/?editorSession=00000000-0000-4000-8000-000000000001";
const second = "/_play/?editorSession=00000000-0000-4000-8000-000000000002";

beforeEach(() => {
  vi.stubGlobal("BroadcastChannel", Channel);
  window.localStorage.setItem("dcl-mcp-relay", "1");
  vi.mocked(autoConnect)
    .mockReturnValueOnce(firstDispose)
    .mockReturnValueOnce(secondDispose);
});

afterEach(() => {
  cleanup();
  vi.clearAllMocks();
  vi.unstubAllGlobals();
  window.localStorage.clear();
});

test("waits for a viewport and reconnects MCP when its transport URL changes", async () => {
  const view = render(<DeWorkspace viewportSrc={null} />);
  await Promise.resolve();
  expect(autoConnect).not.toHaveBeenCalled();

  view.rerender(<DeWorkspace viewportSrc={first} />);
  await waitFor(() => expect(autoConnect).toHaveBeenCalledTimes(1));
  const firstOptions = vi.mocked(autoConnect).mock.calls[0]![0]!;
  expect(firstOptions.getViewportEl?.()?.src).toContain("000000000001");

  view.rerender(<DeWorkspace viewportSrc={second} />);
  await waitFor(() => expect(firstDispose).toHaveBeenCalledOnce());
  await waitFor(() => expect(autoConnect).toHaveBeenCalledTimes(2));
  const secondOptions = vi.mocked(autoConnect).mock.calls[1]![0]!;
  expect(secondOptions.getViewportEl?.()?.src).toContain("000000000002");

  view.unmount();
  expect(secondDispose).toHaveBeenCalledOnce();
});
