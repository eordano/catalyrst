import { afterEach, expect, test, vi } from "vitest";
import { screen, within } from "@testing-library/react";
import { renderHud } from "./harness";
import { makeFriend } from "./fakeBridge";
import * as schema from "../data/catalyst/communitiesSchema";
import * as conversations from "../data/catalyst/conversations";

const alice = "0x1111111111111111111111111111111111111111";
const kai = "0x2222222222222222222222222222222222222222";

afterEach(() => vi.restoreAllMocks());
test("Message on a friend in the Communities panel opens that friend's direct conversation, not the chat in general", async () => {
  vi.spyOn(schema, "loadCommunities").mockResolvedValue([]);
  vi.spyOn(conversations, "loadConversations").mockImplementation(async (kind) =>
    kind === "direct" ? [{ id: kai, name: "Kai" }, { id: alice.toUpperCase().replace("0X", "0x"), name: "Alice" }] : []);
  vi.spyOn(conversations, "loadConversation").mockResolvedValue([]);
  await import("../app/panels/Communities.route");
  await import("../app/panels/Chat.route");
  const { user, path, bridge } = renderHud({ route: "/communities" });
  bridge.pushIdentity({ address: "0x9999999999999999999999999999999999999999", isGuest: false, name: "Morgan" });
  bridge.pushFriends({ friends: [makeFriend({ address: kai, name: "Kai", status: "online" }), makeFriend({ address: alice, name: "Alice", status: "online" })] });
  const rail = await screen.findByRole("navigation", { name: "Sections and communities" });
  await user.click(within(rail).getByRole("button", { name: "Friends" }));
  await user.click(await screen.findByRole("button", { name: "Message Alice" }));
  expect(path()).toBe("/chat");
  expect(await screen.findByRole("textbox", { name: /Alice/ })).toBeInTheDocument();
  expect(screen.queryByRole("textbox", { name: /Kai/ })).not.toBeInTheDocument();
  await user.click(screen.getByRole("button", { name: "Back to direct messages" }));
  expect(await screen.findByRole("button", { name: /Kai/ })).toBeInTheDocument();
});
