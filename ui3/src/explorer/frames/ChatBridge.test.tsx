import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { afterEach, expect, test, vi } from "vitest";
import { FakeBridge, makeFriend } from "../../test/fakeBridge";
import { loadConversations, loadConversation, sendConversationMessage } from "../../data/catalyst/conversations";
import Chat from "./ChatBridge";

vi.mock("../../data/catalyst/conversations", () => ({ loadConversations: vi.fn(), loadConversation: vi.fn(), sendConversationMessage: vi.fn() }));
const alice = "0x1111111111111111111111111111111111111111";
const bob = "0x2222222222222222222222222222222222222222";
afterEach(() => { delete window.dclBridge; vi.resetAllMocks(); });

function mount() {
  vi.mocked(loadConversations).mockImplementation(async (kind) => kind === "direct" ? [{ id: alice, name: "Alice" }, { id: bob, name: "Bob" }] : [{ id: "builders", name: "Builders" }]);
  vi.mocked(loadConversation).mockResolvedValue([{ id: "one", author: alice, body: "Hello there", date: "2026-09-18T12:00:00Z" }]);
  vi.mocked(sendConversationMessage).mockResolvedValue(undefined);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const bridge = new FakeBridge();
  window.dclBridge = bridge;
  const view = render(<QueryClientProvider client={client}><Chat open onToggle={() => {}} /></QueryClientProvider>);
  act(() => {
    bridge.pushIdentity();
    bridge.pushFriends({ friends: [makeFriend({ address: alice, name: "Alice" }), makeFriend({ address: bob, name: "Bob" })] });
  });
  return { view, client, user: userEvent.setup() };
}

test("channel and conversation switches retain drafts, and a failed direct send keeps the text for retry", async () => {
  const { user, client } = mount();
  await user.click(await screen.findByRole("button", { name: "Friends" }));
  await user.type(await screen.findByRole("searchbox", { name: "Search friends" }), "Alice");
  expect(screen.queryByRole("button", { name: /Bob Direct message/ })).toBeNull();
  await user.click(await screen.findByRole("button", { name: /Alice Direct message/ }));
  const composer = () => screen.getByRole("textbox", { name: "Send a message to Alice chat" });
  await user.type(composer(), "draft for Alice");
  await user.click(screen.getByRole("button", { name: "Communities" }));
  await user.click(await screen.findByRole("button", { name: /Builders Community/ }));
  await user.type(screen.getByRole("textbox", { name: "Send a message to Builders chat" }), "community draft");
  await user.click(await screen.findByRole("button", { name: "Friends" }));
  expect(composer()).toHaveValue("draft for Alice");
  expect(composer()).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Back to direct messages" }));
  await user.click(screen.getByRole("button", { name: /Alice Draft saved/ }));
  expect(composer()).toHaveValue("draft for Alice");
  vi.mocked(sendConversationMessage).mockRejectedValueOnce(new Error("Offline. Try again."));
  await user.click(screen.getByRole("button", { name: "Send message to Alice" }));
  expect(await screen.findByRole("alert")).toHaveTextContent("Offline. Try again.");
  expect(composer()).toHaveValue("draft for Alice");
  await user.click(screen.getByRole("button", { name: "Send message to Alice" }));
  await waitFor(() => expect(composer()).toHaveValue(""));
  expect(sendConversationMessage).toHaveBeenLastCalledWith("direct", alice, "draft for Alice");
  await user.type(composer(), "/clear");
  await user.keyboard("{Enter}");
  await waitFor(() => expect(sendConversationMessage).toHaveBeenLastCalledWith("direct", alice, "/clear"));
  client.clear();
});

test("new messages preserve the reader's scroll position until Jump to latest", async () => {
  const { user, client } = mount();
  await user.click(await screen.findByRole("button", { name: "Friends" }));
  await user.click(await screen.findByRole("button", { name: /Alice Direct message/ }));
  await screen.findByText("Hello there");
  const log = screen.getByRole("log", { name: "Alice messages" });
  Object.defineProperties(log, { scrollHeight: { value: 1200, configurable: true }, clientHeight: { value: 300, configurable: true } });
  log.scrollTop = 150;
  fireEvent.scroll(log);
  const query = client.getQueryCache().findAll({ queryKey: ["chat-thread", "direct", alice] })[0]!;
  act(() => client.setQueryData(query.queryKey, [{ id: "one", author: alice, body: "Hello there", date: "2026-09-18T12:00:00Z" }, { id: "two", author: alice, body: "New arrival", date: "2026-09-18T12:01:00Z" }]));
  await screen.findByText("New arrival");
  expect(log.scrollTop).toBe(150);
  await user.click(screen.getByRole("button", { name: "New messages \u00b7 Jump to latest" }));
  expect(log.scrollTop).toBe(1200);
  client.clear();
});
