import { test, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Backpack from "./Backpack";

const CLICK_BUDGET = 1;

const HAT = { urn: "urn:test:hat:1", name: "Cool Hat", category: "hat", rarity: "rare", thumbnail: "" };
const WAVE = { urn: "urn:test:emote:wave", name: "Wave", category: "emote", rarity: "common", thumbnail: "" };

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

test(`wear, recolor and emote each reach the engine in <= ${CLICK_BUDGET} click`, async () => {
  let clicks = 0;
  const click = async (el: Element) => {
    clicks++;
    await userEvent.click(el);
  };

  const onEquippedChange = vi.fn();
  const wear = render(<Backpack catalog={[HAT]} equipped={EQUIPPED} onEquippedChange={onEquippedChange} />);
  await click(screen.getByTitle("Cool Hat"));
  expect(onEquippedChange).toHaveBeenCalledWith(["urn:test:hat:1"]);
  expect(send).toHaveBeenCalledWith(
    "SetAvatar",
    expect.objectContaining({
      equip: expect.objectContaining({ wearableUrns: ["urn:test:hat:1"] }),
    }),
  );
  expect(clicks).toBeLessThanOrEqual(CLICK_BUDGET);
  wear.unmount();

  send.mockClear();
  clicks = 0;
  const recolor = render(<Backpack catalog={[]} equipped={EQUIPPED} />);
  await userEvent.click(screen.getByRole("button", { name: "Hair" }));
  const swatch = recolor.container.querySelector(".bp__swatch");
  expect(swatch, "color swatches should show for a color category").toBeTruthy();
  await click(swatch!);
  expect(send).toHaveBeenCalledWith("SetAvatar", expect.anything());
  expect(clicks).toBeLessThanOrEqual(CLICK_BUDGET);
  recolor.unmount();

  send.mockClear();
  clicks = 0;
  render(<Backpack catalog={[]} emoteCatalog={[WAVE]} equipped={EQUIPPED} />);
  await userEvent.click(screen.getByRole("tab", { name: /Emotes/i }));
  await click(screen.getByTitle("Preview Wave"));
  expect(send).toHaveBeenCalledWith("PlayEmote", { urn: "urn:test:emote:wave" });
  expect(clicks).toBeLessThanOrEqual(CLICK_BUDGET);
});
