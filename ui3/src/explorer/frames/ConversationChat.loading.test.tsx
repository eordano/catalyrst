import { act, fireEvent, render, screen } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import { FakeBridge } from "../../test/fakeBridge";
import type { ConversationMessage } from "../../data/catalyst/conversations";

const api = vi.hoisted(() => ({ list: vi.fn(), thread: vi.fn() }));
vi.mock("../../data/catalyst/conversations", () => ({
  loadConversations: api.list, loadConversation: api.thread, sendConversationMessage: vi.fn(),
}));
vi.mock("./Chat", () => ({ ChatView: ({ io }: { io: { chat: unknown[] } }) => <div>{io.chat.length ? "Messages" : "Empty conversation"}</div> }));
import ConversationChat from "./ConversationChat";

afterEach(() => { delete window.dclBridge; });

test("does not show an empty thread until history succeeds, and retries after a failure", async () => {
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  api.list.mockResolvedValue([{ id: "alice", name: "Alice" }]);
  let reject!: (error: Error) => void;
  api.thread.mockImplementationOnce(() => new Promise<ConversationMessage[]>((_, fail) => { reject = fail; })).mockResolvedValue([]);
  render(<QueryClientProvider client={client}><ConversationChat kind="direct" /></QueryClientProvider>);
  act(() => bridge.pushIdentity());
  fireEvent.click(await screen.findByRole("button", { name: /Alice/ }));
  expect(screen.getByRole("status")).toHaveTextContent("Loading messages");
  expect(screen.queryByText("Empty conversation")).toBeNull();
  await act(async () => reject(new Error("unavailable")));
  expect(await screen.findByRole("alert")).toHaveTextContent("Couldn't load messages");
  expect(screen.queryByText("Empty conversation")).toBeNull();
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(await screen.findByText("Empty conversation")).toBeInTheDocument();
  expect(api.thread).toHaveBeenCalledTimes(2);
});
