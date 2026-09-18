import { describe, test, expect } from "vitest";
import { fireEvent, screen, within } from "@testing-library/react";

import { renderHud } from "./harness";

const sidebar = () => screen.getByRole("navigation", { name: "Main menu" });
const chatInput = () => screen.getByLabelText("Send a message to Nearby chat");

describe("HUD mount contract", () => {
  test("mounting requests the avatar preview and stops emotes, and so does returning from a panel", async () => {
    const { user, bridge } = renderHud();
    bridge.expectSent("RequestAvatarPreview");
    bridge.expectSent("StopEmote");

    await user.keyboard("p");
    bridge.clearSent();
    await user.keyboard("{Escape}");
    bridge.expectSent("StopEmote");
  });
});

describe("chat commands", () => {
  test("Enter sends SendChat {message, channel} and clears the draft; whitespace-only drafts are not sent", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));

    await user.type(chatInput(), "gm nearby{Enter}");
    expect(bridge.expectSent("SendChat")).toEqual({ channel: "Nearby", message: "gm nearby" });
    expect(chatInput()).toHaveValue("");

    bridge.clearSent();
    await user.type(chatInput(), "   {Enter}");
    bridge.expectNotSent("SendChat");
  });

  test("slash commands go to the engine console path; echo and output render as console lines stamped today", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    await user.type(chatInput(), "/fps 30{Enter}");
    expect(bridge.expectSent("SendChat")).toEqual({ channel: "Nearby", message: "/fps 30" });
    expect(screen.queryByText("/fps 30")).toBeNull();

    bridge.pushIdentity({ address: "0xabc0000000000000000000000000000000000abc", name: "Me" });
    bridge.pushChat({ senderName: "Me", senderAddress: "0xabc0000000000000000000000000000000000abc", channel: "System", message: "/fps 30", timestamp: 12.5 });
    bridge.pushChat({ senderName: "", senderAddress: "", channel: "System", message: "Available commands:" });

    const echo = await screen.findByText("/fps 30");
    expect(echo.closest("[data-console]")).toHaveAttribute("data-console", "echo");
    expect(screen.getByText("Available commands:").closest("[data-console]")).toHaveAttribute("data-console", "output");
    expect(screen.queryByRole("button", { name: /^View / })).toBeNull();
    expect(screen.getByText("Today")).toBeInTheDocument();
    expect(screen.queryByText(/Jan 1/)).toBeNull();
  });

  test("/clear answers locally; /help asks the engine and merges its list; /help <cmd> answers from the table", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    bridge.pushChat({ message: "before the clear" });
    expect(await screen.findByText("before the clear")).toBeInTheDocument();

    await user.type(chatInput(), "/clear{Enter}");
    bridge.expectNotSent("SendChat");
    expect(screen.queryByText("before the clear")).toBeNull();

    bridge.pushChat({ message: "after the clear" });
    expect(await screen.findByText("after the clear")).toBeInTheDocument();
    expect(screen.queryByText("before the clear")).toBeNull();

    await user.type(chatInput(), "/help{Enter}");
    expect(bridge.expectSent("SendChat")).toEqual({ channel: "Nearby", message: "/help" });
    expect(screen.queryByText(/Chat commands:/)).toBeNull();
    const engine = { senderName: "", senderAddress: "", channel: "System" as const };
    bridge.pushChat({ ...engine, message: "Available commands:" });
    bridge.pushChat({ ...engine, message: "  /fps             - Set the target frame rate" });
    bridge.pushChat({ ...engine, message: "  /noclip          - " });
    bridge.pushChat({ ...engine, message: "[ok]" });
    const help = await screen.findByText(/Chat commands:/);
    expect(help).toHaveTextContent("/clear");
    expect(help).toHaveTextContent("/fps <fps> \u{2014} set the target frame rate");
    expect(help).toHaveTextContent("/noclip");
    expect(help).not.toHaveTextContent("/teleport");
    expect(help.closest("[data-console]")).toHaveAttribute("data-console", "output");
    expect(screen.queryByText("Available commands:")).toBeNull();
    expect(screen.queryByText("[ok]")).toBeNull();

    bridge.clearSent();
    await user.type(chatInput(), "/help fps{Enter}");
    bridge.expectNotSent("SendChat");
    await user.type(chatInput(), "/help idnoclip{Enter}");
    expect(bridge.expectSent("SendChat")).toEqual({ channel: "Nearby", message: "/help /idnoclip" });
  });

  test("clicking a sender opens their profile card at body level; Mention fills the draft", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Chat" }));
    bridge.pushChat({ senderName: "Ripley", message: "hi there" });

    await user.click(await screen.findByRole("button", { name: "View Ripley" }));
    const card = screen.getByRole("dialog", { name: "Profile" });
    expect(card.parentElement).toBe(document.body);
    expect(within(card).getByText("Ripley")).toBeInTheDocument();
    expect(within(card).getByRole("button", { name: "View Passport" })).toBeInTheDocument();

    await user.click(within(card).getByRole("button", { name: "Mention" }));
    expect(screen.queryByRole("dialog", { name: "Profile" })).toBeNull();
    expect(chatInput()).toHaveValue("@Ripley ");
  });
});

describe("emote wheel", () => {
  test("B toggles the wheel, Escape closes it, and picking an emote plays it without cancelling", async () => {
    const { user, bridge } = renderHud();
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();

    await user.keyboard("b");
    expect(screen.getByRole("button", { name: "Wave" })).toBeInTheDocument();
    await user.keyboard("b");
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();
    await user.keyboard("b");
    await user.keyboard("{Escape}");
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();

    await user.keyboard("b");
    bridge.clearSent();
    await user.click(screen.getByRole("button", { name: "Wave" }));
    expect(bridge.expectSent("PlayEmote")).toEqual({ urn: "urn:decentraland:off-chain:base-emotes:wave" });
    expect(screen.queryByRole("button", { name: "Wave" })).toBeNull();
    bridge.expectNotSent("StopEmote");
  });

  test("each slot maps to its own urn, by click and by digit hotkey (1-9 then 0)", async () => {
    const { user, bridge } = renderHud();
    await user.keyboard("b");
    await user.click(screen.getByRole("button", { name: "Clap" }));
    expect(bridge.expectSent("PlayEmote")).toEqual({ urn: "urn:decentraland:off-chain:base-emotes:clap" });

    await user.keyboard("b");
    await user.keyboard("3");
    expect(bridge.expectSent("PlayEmote")).toEqual({ urn: "urn:decentraland:off-chain:base-emotes:dance" });
    expect(screen.queryByRole("button", { name: "Dance" })).toBeNull();

    await user.keyboard("b");
    await user.keyboard("0");
    expect(bridge.expectSent("PlayEmote")).toEqual({ urn: "urn:decentraland:off-chain:base-emotes:disco" });
    expect(bridge.sentOf("PlayEmote")).toHaveLength(3);
  });
});

describe("settings", () => {
  test("a slider commits one SetSetting with the raw engine value on release", async () => {
    const { user, bridge, navigate } = renderHud();
    await navigate("/settings");
    await user.click(await screen.findByRole("tab", { name: "Sound" }));

    const slider = screen.getByLabelText("Master");
    fireEvent.change(slider, { target: { value: "70" } });
    fireEvent.change(slider, { target: { value: "62" } });
    bridge.expectNotSent("SetSetting");
    fireEvent.pointerUp(slider);
    expect(bridge.expectSent("SetSetting")).toEqual({ name: "Master Volume", value: 62 });
    expect(bridge.sentOf("SetSetting")).toHaveLength(1);
  });

  test("the panel pulls a snapshot, renders engine values, and dropdowns send the mapped engine value", async () => {
    const { user, bridge, navigate } = renderHud();
    await navigate("/settings");
    bridge.expectSent("GetSettings");

    await user.click(await screen.findByRole("button", { name: "FPS Limit" }));
    await user.click(screen.getByRole("option", { name: "60 fps" }));
    expect(bridge.expectSent("SetSetting")).toEqual({ name: "Target Frame Rate", value: 4 });

    bridge.push({
      kind: "settings",
      settings: [
        {
          name: "Bloom",
          category: "Graphics",
          description: "Glow around bright light sources.",
          minValue: 0,
          maxValue: 3,
          namedVariants: [
            { name: "Off", description: "" },
            { name: "Low", description: "" },
            { name: "High", description: "" },
          ],
          stepSize: 1,
          value: 0,
          default: 1,
        },
      ],
    });
    const bloom = await screen.findByRole("button", { name: "Bloom" });
    expect(bloom).toHaveTextContent("Off");
    await user.click(bloom);
    await user.click(screen.getByRole("option", { name: "High" }));
    expect(bridge.expectSent("SetSetting", { name: "Bloom" })).toEqual({ name: "Bloom", value: 2 });
  });
});

describe("voice and skybox panels", () => {
  test("Speak toggles SetMic from the current mic state and the volume slider sends Voice Volume", async () => {
    const { user, bridge } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Voice Chat" }));

    await user.click(screen.getByRole("button", { name: "Speak" }));
    expect(bridge.expectSent("SetMic")).toEqual({ enabled: true });

    bridge.pushMic({ enabled: true });
    await user.click(screen.getByRole("button", { name: "Mic on \u{2014} click to mute" }));
    expect(bridge.expectSent("SetMic", { enabled: false })).toEqual({ enabled: false });

    fireEvent.change(screen.getByLabelText("Nearby voice volume"), { target: { value: "70" } });
    expect(bridge.expectSent("SetSetting")).toEqual({ name: "Voice Volume", value: 70 });
  });

  test("disabling auto and sliding time sends SetTimeOfDay", async () => {
    const { user, bridge, container } = renderHud();
    await user.click(within(sidebar()).getByRole("button", { name: "Skybox" }));

    await user.click(screen.getByRole("switch"));
    expect(bridge.expectSent("SetTimeOfDay")).toEqual({ minutes: 990, auto: false });

    const range = container.querySelector<HTMLInputElement>(".sky__range");
    expect(range).not.toBeNull();
    fireEvent.change(range as HTMLInputElement, { target: { value: "720" } });
    expect(bridge.expectSent("SetTimeOfDay", { minutes: 720 })).toEqual({ minutes: 720, auto: false });
  });
});
