import { describe, test, expect } from "vitest";
import { screen, within } from "@testing-library/react";

import { makeFriend, renderHud } from "./harness";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });
const connBadge = () => screen.getByRole("button", { name: "Connection status" });

describe("identity pushes", () => {
  test("pushIdentity fills the profile widget with name, tag and wallet", async () => {
    const { user, bridge } = renderHud();
    bridge.pushIdentity({
      name: "Ada",
      tag: "4242",
      address: "0x1234567890abcdef1234567890abcdef12345678",
      isGuest: false,
    });

    await user.click(within(sidebar()).getByRole("button", { name: "Profile" }));
    expect(screen.getByText("Ada")).toBeInTheDocument();
    expect(screen.getByText("(4242)")).toBeInTheDocument();
    expect(screen.getByText(/0x123\u{2026}5678/u)).toBeInTheDocument();
    expect(screen.queryByText("Sign in")).toBeNull();
  });

  test("guest identity shows the Sign in entry and no wallet", async () => {
    const { user, bridge } = renderHud();
    bridge.pushIdentity({ name: "Guest-77", isGuest: true, address: "" });

    await user.click(within(sidebar()).getByRole("button", { name: "Profile" }));
    expect(screen.getByText("Guest-77")).toBeInTheDocument();
    expect(screen.getByText("Sign in")).toBeInTheDocument();
    expect(screen.queryByText("WALLET ADDRESS")).toBeNull();
  });
});

describe("scene pushes", () => {
  test("pushScene drives the minimap title and coordinates", () => {
    const { bridge } = renderHud();
    bridge.pushScene({ title: "Tower of Hanoi", coords: "62,-8" });
    expect(screen.getByText("Tower of Hanoi")).toBeInTheDocument();
    expect(screen.getByText(/62,-8/)).toBeInTheDocument();
  });

  test("later scene pushes replace the minimap values", () => {
    const { bridge } = renderHud();
    bridge.pushScene({ title: "First Place", coords: "1,1" });
    bridge.pushScene({ title: "Second Place", coords: "2,2" });
    expect(screen.queryByText("First Place")).toBeNull();
    expect(screen.getByText("Second Place")).toBeInTheDocument();
  });
});

describe("friends pushes", () => {
  test("online friends light the sidebar presence dot", () => {
    const { bridge, container } = renderHud();
    const friendsBtn = within(sidebar()).getByRole("button", { name: "Friends" });
    expect(friendsBtn.querySelector(".sb__notif")).toBeNull();

    bridge.pushFriends({ friends: [makeFriend({ status: "online" })] });
    expect(
      within(sidebar())
        .getByRole("button", { name: "Friends" })
        .querySelector(".sb__notif"),
    ).not.toBeNull();
    expect(container).toBeInTheDocument();
  });

  test("the Friends panel lists pushed friends with online/offline groups", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Friends" }));
    // The panel is lazy -- wait for it to mount (and subscribe) before pushing, and sign in:
    // the reconciled Friends panel gates its roster behind a non-guest identity.
    await screen.findByRole("tab", { name: "Friends" });
    bridge.pushIdentity({ isGuest: false });

    bridge.pushFriends({
      friends: [
        makeFriend({ name: "Ripley", status: "online", address: "0x" + "1".repeat(40) }),
        makeFriend({ name: "Hicks", status: "offline", address: "0x" + "2".repeat(40) }),
      ],
    });

    expect(screen.getByText("Ripley")).toBeInTheDocument();
    expect(screen.getByText("Hicks")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Online \(1\)/ })).toBeInTheDocument();
    expect(screen.getByRole("button", { name: /Offline \(1\)/ })).toBeInTheDocument();
  });
});

describe("connection pushes", () => {
  test("badge is neutral before the first push, ok when healthy, warn on errors", () => {
    const { bridge } = renderHud();
    expect(connBadge().className).toContain("connbadge--info");

    bridge.pushConnection({ sceneHealth: "ok", globalRoom: true });
    expect(connBadge().className).toContain("connbadge--ok");

    bridge.pushConnection({ sceneHealth: "error", globalRoom: true });
    expect(connBadge().className).toContain("connbadge--warn");

    bridge.pushConnection({ sceneHealth: "ok", globalRoom: false });
    expect(connBadge().className).toContain("connbadge--warn");
  });

  test("the connection dialog shows honest placeholders before any push", async () => {
    const { user } = renderHud();
    await user.click(connBadge());
    const dialog = screen.getByRole("dialog", { name: "Connection status" });
    expect(within(dialog).getAllByText("\u{2026}")).toHaveLength(4);
  });

  test("the connection dialog rows reflect real push values", async () => {
    const { user, bridge } = renderHud();
    bridge.pushScene({ realm: "hela" });
    bridge.pushConnection({ sceneHealth: "ok", sceneRoom: false, globalRoom: true });

    await user.click(connBadge());
    const dialog = screen.getByRole("dialog", { name: "Connection status" });
    expect(within(dialog).getByText("Healthy")).toBeInTheDocument();
    expect(within(dialog).getByText("None")).toBeInTheDocument();
    expect(within(dialog).getByText("Connected")).toBeInTheDocument();
    expect(within(dialog).getByText("hela")).toBeInTheDocument();

    bridge.pushConnection({ sceneHealth: "error", sceneRoom: true, globalRoom: false });
    expect(within(dialog).getByText("Errors")).toBeInTheDocument();
    expect(within(dialog).getByText("Connected")).toBeInTheDocument();
    expect(within(dialog).getByText("Disconnected")).toBeInTheDocument();
  });

  test("the connection dialog close button dismisses it", async () => {
    const { user } = renderHud();
    await user.click(connBadge());
    const dialog = screen.getByRole("dialog", { name: "Connection status" });
    await user.click(within(dialog).getByRole("button", { name: "Close" }));
    expect(screen.queryByRole("dialog", { name: "Connection status" })).toBeNull();
  });
});

describe("chat pushes", () => {
  test("chat starts honest-empty when live, then shows pushed lines", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    expect(
      screen.getByText("No messages yet \u{2014} say hello to Nearby."),
    ).toBeInTheDocument();

    bridge.pushChat({ senderName: "Ripley", message: "gm nearby" });
    expect(screen.getByText("Ripley")).toBeInTheDocument();
    expect(screen.getByText("gm nearby")).toBeInTheDocument();
    expect(
      screen.queryByText("No messages yet \u{2014} say hello to Nearby."),
    ).toBeNull();
  });

  test("a sender without a name falls back to the short address", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    bridge.pushChat({
      senderName: "",
      senderAddress: "0xabcdef1234567890abcdef1234567890abcdef12",
      message: "who am i",
    });
    expect(screen.getByText("0xabcd\u{2026}ef12")).toBeInTheDocument();
  });

  test("multiple chat pushes accumulate in order", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    bridge.pushChat({ message: "first", timestamp: 1 });
    bridge.pushChat({ message: "second", timestamp: 2 });
    const texts = screen.getAllByText(/^(first|second)$/).map((n) => n.textContent);
    expect(texts).toEqual(["first", "second"]);
  });

  test("a blocked sender's messages hide retroactively and return on unblock", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    const griefer = "0x" + "b".repeat(40);
    bridge.pushChat({
      senderName: "Griefer",
      senderAddress: griefer,
      message: "spam",
      timestamp: 1,
    });
    expect(screen.getByText("spam")).toBeInTheDocument();

    bridge.pushFriends({ blocked: [griefer] });
    expect(screen.queryByText("spam")).toBeNull();

    bridge.pushChat({
      senderName: "Pal",
      senderAddress: "0x" + "c".repeat(40),
      message: "hello",
      timestamp: 2,
    });
    expect(screen.getByText("hello")).toBeInTheDocument();

    bridge.pushFriends({ blocked: [] });
    expect(screen.getByText("spam")).toBeInTheDocument();
  });
});

describe("login code pushes", () => {
  test("a loginCode push opens the sign-in modal with the real code", () => {
    const { bridge } = renderHud();
    bridge.pushLoginCode({ code: 77 });
    const modal = screen.getByRole("dialog", { name: /Sign in/i });
    expect(within(modal).getByText("77")).toBeInTheDocument();
  });

  test("a signed-in identity push clears the login code modal", () => {
    const { bridge } = renderHud();
    bridge.pushLoginCode({ code: 77 });
    expect(screen.getByRole("dialog", { name: /Sign in/i })).toBeInTheDocument();

    bridge.pushIdentity({ isGuest: false });
    expect(screen.queryByRole("dialog", { name: /Sign in/i })).toBeNull();
  });

  test("mic pushes drive the voice panel state", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    expect(screen.getByRole("button", { name: "Speak" })).toHaveAttribute(
      "aria-pressed",
      "false",
    );

    bridge.pushMic({ enabled: true });
    expect(
      screen.getByRole("button", { name: "Mic on \u{2014} click to mute" }),
    ).toHaveAttribute("aria-pressed", "true");
    expect(
      within(sidebar())
        .getByRole("button", { name: "Voice Chat" })
        .querySelector(".sb__presence"),
    ).not.toBeNull();
  });
});
