import { describe, test, expect } from "vitest";
import { screen, within } from "@testing-library/react";

import { makeFriend, renderHud } from "./harness";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });
const connBadge = () => screen.getByRole("button", { name: "Connection status" });

describe("identity and scene pushes", () => {
  test("the lobby account menu reflects the current identity and offers guests Sign in", async () => {
    const signedIn = renderHud();
    signedIn.bridge.pushIdentity({ name: "Ada", tag: "4242", address: "0x1234567890abcdef1234567890abcdef12345678", isGuest: false });
    await signedIn.user.click(within(sidebar()).getByRole("button", { name: "Open lobby" }));
    await signedIn.user.click(await screen.findByRole("button", { name: "Account menu" }));
    expect(within(screen.getByRole("dialog", { name: "Your account" })).getByText("Ada")).toBeInTheDocument();
    expect(screen.queryByText("Sign in")).toBeNull();
    signedIn.unmount();

    const { user, bridge } = renderHud();
    bridge.pushIdentity({ name: "Guest-77", isGuest: true, address: "" });
    await user.click(within(sidebar()).getByRole("button", { name: "Open lobby" }));
    await user.click(await screen.findByRole("button", { name: "Account menu" }));
    expect(within(screen.getByRole("dialog", { name: "Your account" })).getByText("Guest-77")).toBeInTheDocument();
    expect(screen.getByText("Sign in")).toBeInTheDocument();
    expect(screen.queryByText("WALLET ADDRESS")).toBeNull();
  });

  test("scene pushes drive the minimap title and coordinates, later pushes replacing earlier ones", () => {
    const { bridge } = renderHud();
    bridge.pushScene({ title: "Tower of Hanoi", coords: "62,-8" });
    expect(screen.getByText("Tower of Hanoi")).toBeInTheDocument();
    expect(screen.getByText(/62,-8/)).toBeInTheDocument();

    bridge.pushScene({ title: "Second Place", coords: "2,2" });
    expect(screen.queryByText("Tower of Hanoi")).toBeNull();
    expect(screen.getByText("Second Place")).toBeInTheDocument();
  });
});

describe("friends pushes", () => {
  test("online friends light the sidebar presence dot and the panel groups them online/offline", async () => {
    const { user, bridge } = renderHud();
    const friendsBtn = () => within(sidebar()).getByRole("button", { name: "Friends" });
    expect(friendsBtn().querySelector(".sb__notif")).toBeNull();

    await user.click(friendsBtn());
    await screen.findByRole("tab", { name: "Friends" });
    bridge.pushIdentity({ isGuest: false });
    bridge.pushFriends({
      friends: [
        makeFriend({ name: "Ripley", status: "online", address: "0x" + "1".repeat(40) }),
        makeFriend({ name: "Hicks", status: "offline", address: "0x" + "2".repeat(40) }),
      ],
    });
    expect(friendsBtn().querySelector(".sb__notif")).not.toBeNull();
    expect(screen.getByText("Ripley")).toBeInTheDocument();
    expect(screen.getByText("Hicks")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Online \(1\)/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Offline \(1\)/ })).toBeInTheDocument();
  });
});

describe("connection pushes", () => {
  test("the connection dialog shows honest placeholders before any push, real values after, and closes", async () => {
    const { user, bridge } = renderHud();
    await user.click(connBadge());
    const dialog = screen.getByRole("dialog", { name: "Connection status" });
    expect(within(dialog).getByRole("tab", { name: "Performance" })).toHaveAttribute("aria-selected", "true");
    await user.click(within(dialog).getByRole("tab", { name: "Connection" }));
    expect(within(dialog).getAllByText("\u{2026}").length).toBeGreaterThan(0);

    bridge.pushScene({ realm: "hela" });
    bridge.pushConnection({ sceneHealth: "ok", sceneRoom: false, globalRoom: true });
    expect(within(dialog).getByText("Healthy")).toBeInTheDocument();
    expect(within(dialog).getByText("None")).toBeInTheDocument();
    expect(within(dialog).getByText("Connected")).toBeInTheDocument();
    expect(within(dialog).getAllByText("hela").length).toBeGreaterThan(0);

    bridge.pushConnection({ sceneHealth: "error", sceneRoom: true, globalRoom: false });
    expect(within(dialog).getByText("Errors")).toBeInTheDocument();
    expect(within(dialog).getByText("Connected")).toBeInTheDocument();
    expect(within(dialog).getByText("Disconnected")).toBeInTheDocument();

    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Connection status" })).toBeNull();
  });
});

describe("chat pushes", () => {
  test("chat starts honest-empty, then accumulates pushed lines in order, naming anonymous senders by short address", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    expect(screen.getByText("Say hello to people nearby!")).toBeInTheDocument();

    bridge.pushChat({ senderName: "Ripley", message: "first", timestamp: 1 });
    bridge.pushChat({ senderName: "", senderAddress: "0xabcdef1234567890abcdef1234567890abcdef12", message: "second", timestamp: 2 });
    expect(screen.queryByText("Say hello to people nearby!")).toBeNull();
    expect(screen.getByText("Ripley")).toBeInTheDocument();
    expect(screen.getByText("0xabcd\u{2026}ef12")).toBeInTheDocument();
    expect(screen.getAllByText(/^(first|second)$/).map((n) => n.textContent)).toEqual(["first", "second"]);
  });

  test("a blocked sender's messages hide retroactively and return on unblock", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    const griefer = "0x" + "b".repeat(40);
    bridge.pushChat({ senderName: "Griefer", senderAddress: griefer, message: "spam", timestamp: 1 });
    expect(screen.getByText("spam")).toBeInTheDocument();

    bridge.pushFriends({ blocked: [griefer] });
    expect(screen.queryByText("spam")).toBeNull();

    bridge.pushChat({ senderName: "Pal", senderAddress: "0x" + "c".repeat(40), message: "hello", timestamp: 2 });
    expect(screen.getByText("hello")).toBeInTheDocument();

    bridge.pushFriends({ blocked: [] });
    expect(screen.getByText("spam")).toBeInTheDocument();
  });
});

describe("login code and mic pushes", () => {
  test("a loginCode push opens the sign-in modal with the real code; a signed-in identity clears it", () => {
    const { bridge } = renderHud();
    bridge.pushLoginCode({ code: 77 });
    const modal = screen.getByRole("dialog", { name: /Sign in/i });
    expect(within(modal).getByText("77")).toBeInTheDocument();

    bridge.pushIdentity({ isGuest: false });
    expect(screen.queryByRole("dialog", { name: /Sign in/i })).toBeNull();
  });

  test("mic pushes drive the voice panel state and the sidebar presence dot", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.getByRole("button", { name: "Speak" })).toHaveAttribute("aria-pressed", "false");

    bridge.pushMic({ enabled: true });
    expect(screen.getByRole("button", { name: "Mic on \u{2014} click to mute" })).toHaveAttribute("aria-pressed", "true");
    expect(within(sidebar()).getByRole("button", { name: "Voice Chat" }).querySelector(".sb__presence")).not.toBeNull();
  });
});


test("chat shows Friends only with friends and returns to Nearby when the last friend is removed", async () => {
  const { user, bridge } = renderHud();
  await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
  const channels = within(screen.getByRole("group", { name: "Chat channels" }));
  expect(channels.queryByRole("button", { name: "Friends" })).toBeNull();
  expect(channels.queryByRole("button", { name: "Messages" })).toBeNull();
  bridge.pushFriends({ friends: [makeFriend({ name: "Ada", status: "offline" })] });
  await user.click(channels.getByRole("button", { name: "Friends" }));
  expect(channels.getByRole("button", { name: "Friends" })).toHaveAttribute("aria-pressed", "true");
  bridge.pushFriends({ friends: [] });
  expect(channels.queryByRole("button", { name: "Friends" })).toBeNull();
  expect(channels.getByRole("button", { name: "Nearby" })).toHaveAttribute("aria-pressed", "true");
  expect(screen.getByLabelText("Send a message to Nearby chat")).toBeVisible();
});
