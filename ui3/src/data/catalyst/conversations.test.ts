import { beforeEach, expect, test, vi } from "vitest";
import { sendSignedJSON } from "./client";
import { loadConversation, loadConversations, sendConversationMessage } from "./conversations";
vi.mock("./client", () => ({ sendSignedJSON: vi.fn() }));
const request = vi.mocked(sendSignedJSON);
beforeEach(() => request.mockReset());

test("loads signed friend threads and sends the backend's message shape", async () => {
  request.mockResolvedValueOnce({ friends: [{ address: "alice", name: "Alice" }] });
  expect(await loadConversations("direct")).toEqual([{ id: "alice", name: "Alice" }]);
  request.mockResolvedValueOnce({ messages: [{ id: "1", from: "alice", body: "hello", sentAt: "2026-09-18T00:00:00Z" }] });
  expect(await loadConversation("direct", "alice")).toEqual([{ id: "1", author: "alice", body: "hello", date: "2026-09-18T00:00:00Z" }]);
  request.mockResolvedValueOnce({ message: { id: "2" } });
  await sendConversationMessage("direct", "alice", "Hi");
  expect(request).toHaveBeenLastCalledWith("/v1/friends/alice/messages", { body: { body: "Hi" } });
});

test("lists memberships, orders community posts oldest first, and preserves API failures", async () => {
  request.mockResolvedValueOnce({ data: { results: [{ id: "community", name: "Builders" }] } });
  expect(await loadConversations("community")).toEqual([{ id: "community", name: "Builders" }]);
  expect(request).toHaveBeenLastCalledWith("/v1/communities", { method: "GET", query: { onlyMemberOf: true, limit: 100 } });
  request.mockResolvedValueOnce({ data: { posts: [{ id: "2", content: "second" }, { id: "1", content: "first" }] } });
  expect((await loadConversation("community", "community")).map((m) => m.id)).toEqual(["1", "2"]);
  request.mockRejectedValueOnce(new Error("Not a member"));
  await expect(sendConversationMessage("community", "community", "Hi")).rejects.toThrow("Not a member");
  request.mockResolvedValueOnce({});
  await expect(sendConversationMessage("community", "community", "Hi")).rejects.toThrow("did not confirm");
});
