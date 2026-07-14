import { test, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Backpack from "./Backpack";

const EMOTE_CLICK_BUDGET = 1;

const WAVE = {
  urn: "urn:test:emote:wave",
  name: "Wave",
  category: "emote",
  rarity: "common",
  thumbnail: "",
};

const EQUIPPED = {
  wearables: [],
  bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
  skinColor: "#c98c63",
  hairColor: "#5c3824",
  eyeColor: "#3a6ea5",
  emotes: [],
};

const send = vi.fn();
type WinWithBridge = { dclBridge?: { send: typeof send } };

beforeEach(() => {
  send.mockClear();
  (window as unknown as WinWithBridge).dclBridge = { send };
});
afterEach(() => {
  delete (window as unknown as WinWithBridge).dclBridge;
});

test(`emote plays in <= ${EMOTE_CLICK_BUDGET} click (tile -> PlayEmote)`, async () => {
  render(<Backpack catalog={[]} emoteCatalog={[WAVE]} equipped={EQUIPPED} />);

  await userEvent.click(screen.getByRole("tab", { name: /Emotes/i }));

  let clicks = 0;
  const click = async (el: Element) => {
    clicks++;
    await userEvent.click(el);
  };

  await click(screen.getByTitle("Preview Wave"));

  expect(send).toHaveBeenCalledWith("PlayEmote", { urn: "urn:test:emote:wave" });
  expect(clicks).toBeLessThanOrEqual(EMOTE_CLICK_BUDGET);
});
