import { test, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Backpack from "./Backpack";

const RECOLOR_CLICK_BUDGET = 1;

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

test(`recolor is <= ${RECOLOR_CLICK_BUDGET} click (swatch -> SetAvatar)`, async () => {
  const { container } = render(<Backpack catalog={[]} equipped={EQUIPPED} />);

  await userEvent.click(screen.getByRole("button", { name: "Hair" }));
  const swatch = container.querySelector(".bp__swatch");
  expect(swatch, "color swatches should show for a color category").toBeTruthy();

  let clicks = 0;
  const click = async (el: Element) => {
    clicks++;
    await userEvent.click(el);
  };

  await click(swatch!);

  expect(send).toHaveBeenCalledWith("SetAvatar", expect.anything());
  expect(clicks).toBeLessThanOrEqual(RECOLOR_CLICK_BUDGET);
});
