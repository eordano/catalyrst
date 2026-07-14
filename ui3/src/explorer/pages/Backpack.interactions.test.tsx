import { vi, test, expect, beforeEach, afterEach } from "vitest";
import { render } from "@testing-library/react";
import { composeStories } from "@storybook/react";
import * as BackpackStories from "./Backpack.interactions.stories";

const { EquipWearable, FilterByCategory } = composeStories(BackpackStories);

const send = vi.fn();
type WinWithBridge = { dclBridge?: { send: typeof send } };

beforeEach(() => {
  send.mockClear();
  (window as unknown as WinWithBridge).dclBridge = { send };
});
afterEach(() => {
  delete (window as unknown as WinWithBridge).dclBridge;
});

test("Backpack: equipping a wearable pushes SetAvatar to the engine bridge", async () => {
  const { container } = render(<EquipWearable />);
  await EquipWearable.play?.({ canvasElement: container });

  expect(send).toHaveBeenCalledWith(
    "SetAvatar",
    expect.objectContaining({
      equip: expect.objectContaining({ wearableUrns: ["urn:test:hat:1"] }),
    }),
  );
});

test("Backpack: category rail filters the grid to the selected category", async () => {
  const { container } = render(<FilterByCategory />);
  await FilterByCategory.play?.({ canvasElement: container });
});
