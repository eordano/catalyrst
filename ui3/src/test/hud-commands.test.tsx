import { describe, test, expect } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";

import { renderHud } from "./harness";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });

describe("HUD mount contract", () => {
  test("mounting the world HUD requests the avatar preview and stops emotes", () => {
    const { bridge } = renderHud();
    bridge.expectSent("RequestAvatarPreview");
    bridge.expectSent("StopEmote");
  });

  test("returning to the world from a panel stops any playing emote", async () => {
    const { user, bridge } = renderHud();
    await user.keyboard("p");
    bridge.clearSent();
    await user.keyboard("{Escape}");
    bridge.expectSent("StopEmote");
  });
});

describe("chat commands", () => {
  test("Enter in the chat input sends SendChat {message, channel} and clears it", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    const input = screen.getByLabelText("Send a message to Nearby chat");

    await user.type(input, "gm nearby{Enter}");
    expect(bridge.expectSent("SendChat")).toEqual({
      channel: "Nearby",
      message: "gm nearby",
    });
    expect(input).toHaveValue("");
  });

  test("whitespace-only chat drafts are not sent", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    await user.type(
      screen.getByLabelText("Send a message to Nearby chat"),
      "   {Enter}",
    );
    bridge.expectNotSent("SendChat");
  });
});

describe("emote wheel", () => {
  test("B toggles the wheel and Escape closes it", async () => {
    const { user } = renderHud();
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();

    await user.keyboard("b");
    expect(screen.getByRole("button", { name: "Wave" })).toBeInTheDocument();

    await user.keyboard("b");
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();

    await user.keyboard("b");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();
  });

  test("picking an emote sends PlayEmote, closes the wheel and does NOT cancel it", async () => {
    const { user, bridge } = renderHud();
    await user.keyboard("b");
    bridge.clearSent();

    await user.click(screen.getByRole("button", { name: "Wave" }));
    expect(bridge.expectSent("PlayEmote")).toEqual({
      urn: "urn:decentraland:off-chain:base-emotes:wave",
    });
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();
    bridge.expectNotSent("StopEmote");
  });

  test("each wheel slot maps to its own urn", async () => {
    const { user, bridge } = renderHud();
    await user.keyboard("b");
    await user.click(screen.getByRole("button", { name: "Clap" }));
    expect(bridge.expectSent("PlayEmote")).toEqual({
      urn: "urn:decentraland:off-chain:base-emotes:clap",
    });

    await user.keyboard("b");
    await user.click(screen.getByRole("button", { name: "Disco" }));
    expect(bridge.expectSent("PlayEmote")).toEqual({
      urn: "urn:decentraland:off-chain:base-emotes:disco",
    });
    expect(bridge.sentOf("PlayEmote")).toHaveLength(2);
  });

  test("digit hotkeys play slots while the wheel is open (1-9 then 0)", async () => {
    const { user, bridge } = renderHud();
    await user.keyboard("b");
    await user.keyboard("3");
    expect(bridge.expectSent("PlayEmote")).toEqual({
      urn: "urn:decentraland:off-chain:base-emotes:dance",
    });
    expect(screen.queryByRole("button", { name: "Dance" })).toBeNull();

    await user.keyboard("b");
    await user.keyboard("0");
    expect(bridge.expectSent("PlayEmote")).toEqual({
      urn: "urn:decentraland:off-chain:base-emotes:disco",
    });
  });
});

describe("settings", () => {
  test("a settings slider sends SetSetting {name, value}", async () => {
    const { user, bridge, navigate } = renderHud();
    await navigate("/settings");
    await user.click(await screen.findByRole("tab", { name: "Sounds" }));

    const slider = screen.getByLabelText("Master volume");
    fireEvent.change(slider, { target: { value: "62" } });
    expect(bridge.expectSent("SetSetting")).toEqual({
      name: "Master Volume",
      value: 62,
    });
  });

  test("a settings toggle maps on/off to its engine values", async () => {
    const { user, bridge, navigate } = renderHud();
    await navigate("/settings");
    const bloom = await screen.findByRole("switch", { name: "Bloom" });
    await user.click(bloom);
    expect(bridge.expectSent("SetSetting")).toEqual({ name: "Bloom", value: 0 });

    await user.click(bloom);
    expect(bridge.expectSent("SetSetting")).toEqual({ name: "Bloom", value: 2 });
  });

  test("a settings dropdown sends the mapped engine value", async () => {
    const { user, bridge, navigate } = renderHud();
    await navigate("/settings");
    await user.click(await screen.findByRole("button", { name: "FPS Limit" }));
    await user.click(screen.getByRole("option", { name: "60" }));
    expect(bridge.expectSent("SetSetting")).toEqual({
      name: "Target Frame Rate",
      value: 4,
    });
  });

  test("a slider with a scale multiplies before sending", async () => {
    const { bridge, navigate } = renderHud();
    await navigate("/settings");
    const slider = await screen.findByLabelText("Shadow distance");
    fireEvent.change(slider, { target: { value: "50" } });
    expect(bridge.expectSent("SetSetting")).toEqual({
      name: "Shadow Distance",
      value: 150,
    });
  });
});

describe("voice", () => {
  test("the Speak button toggles SetMic based on current mic state", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));

    await user.click(screen.getByRole("button", { name: "Speak" }));
    expect(bridge.expectSent("SetMic")).toEqual({ enabled: true });

    bridge.pushMic({ enabled: true });
    await user.click(screen.getByRole("button", { name: "Mic on — click to mute" }));
    expect(bridge.expectSent("SetMic")).toEqual({ enabled: false });
  });

  test("the voice volume slider sends SetSetting Voice Volume", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));
    fireEvent.change(screen.getByLabelText("Nearby voice volume"), {
      target: { value: "70" },
    });
    expect(bridge.expectSent("SetSetting")).toEqual({
      name: "Voice Volume",
      value: 70,
    });
  });
});

describe("skybox", () => {
  test("disabling auto and sliding time sends SetTimeOfDay", async () => {
    const { user, bridge, container } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Skybox" }));

    await user.click(screen.getByRole("switch"));
    expect(bridge.expectSent("SetTimeOfDay")).toEqual({ minutes: 990, auto: false });

    const range = container.querySelector<HTMLInputElement>(".sky__range");
    expect(range).not.toBeNull();
    fireEvent.change(range as HTMLInputElement, { target: { value: "720" } });
    expect(bridge.expectSent("SetTimeOfDay")).toEqual({ minutes: 720, auto: false });
  });
});

describe("minimap teleport", () => {
  test("Jump to coordinates sends Teleport at the parcel center", async () => {
    const { user, bridge } = renderHud();
    bridge.pushScene({ coords: "10,-3" });

    await user.click(screen.getByRole("button", { name: "Scene options" }));
    await user.click(screen.getByRole("menuitem", { name: "Jump to coordinates" }));
    expect(bridge.expectSent("Teleport")).toEqual({ x: 168, z: -40 });
  });

  test("no Teleport is sent when the scene has no coordinates yet", async () => {
    const { user, bridge } = renderHud();
    await user.click(screen.getByRole("button", { name: "Scene options" }));
    await user.click(screen.getByRole("menuitem", { name: "Jump to coordinates" }));
    bridge.expectNotSent("Teleport");
  });
});
