import { test, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Backpack from "./Backpack";

const WEAR_CLICK_BUDGET = 1;

const HAT = {
  urn: "urn:test:hat:1",
  name: "Cool Hat",
  category: "hat",
  rarity: "rare",
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

test(`wear is <= ${WEAR_CLICK_BUDGET} click in the backpack (tile -> equipped + SetAvatar)`, async () => {
  const onEquippedChange = vi.fn();
  render(
    <Backpack
      catalog={[HAT]}
      equipped={EQUIPPED}
      onEquippedChange={onEquippedChange}
    />,
  );

  let clicks = 0;
  const click = async (el: Element) => {
    clicks++;
    await userEvent.click(el);
  };

  await click(screen.getByTitle("Cool Hat"));

  expect(onEquippedChange).toHaveBeenCalledWith(["urn:test:hat:1"]);
  expect(send).toHaveBeenCalledWith(
    "SetAvatar",
    expect.objectContaining({
      equip: expect.objectContaining({ wearableUrns: ["urn:test:hat:1"] }),
    }),
  );
  expect(clicks).toBeLessThanOrEqual(WEAR_CLICK_BUDGET);
});
