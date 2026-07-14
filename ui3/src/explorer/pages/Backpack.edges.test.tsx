import { test, expect, vi, beforeEach, afterEach } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import Backpack from "./Backpack";


const HAT = { urn: "urn:test:hat:1", name: "Cool Hat", category: "hat", rarity: "rare", thumbnail: "" };
const HAT2 = { urn: "urn:test:hat:2", name: "Party Hat", category: "hat", rarity: "rare", thumbnail: "" };

const BASE = {
  wearables: [] as string[],
  bodyShape: "urn:decentraland:off-chain:base-avatars:BaseMale",
  skinColor: "#c98c63",
  hairColor: "#5c3824",
  eyeColor: "#3a6ea5",
  emotes: [] as string[],
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

test("empty catalog renders gracefully (no tiles, no crash)", () => {
  render(<Backpack catalog={[]} equipped={BASE} />);
  expect(screen.getByRole("heading", { name: "Backpack" })).toBeInTheDocument();
  expect(document.querySelectorAll("[role=listitem]").length).toBe(0);
});

test("unequip: clicking an equipped tile toggles it off", async () => {
  const onEquippedChange = vi.fn();
  render(
    <Backpack
      catalog={[HAT]}
      equipped={{ ...BASE, wearables: [HAT.urn] }}
      onEquippedChange={onEquippedChange}
    />,
  );
  await userEvent.click(screen.getByTitle("Cool Hat"));
  expect(onEquippedChange).toHaveBeenCalledWith([]);
  expect(send).toHaveBeenCalledWith(
    "SetAvatar",
    expect.objectContaining({
      equip: expect.objectContaining({ wearableUrns: [] }),
    }),
  );
});

test("same-category replace: equipping a 2nd hat replaces the first", async () => {
  const onEquippedChange = vi.fn();
  render(
    <Backpack
      catalog={[HAT, HAT2]}
      equipped={{ ...BASE, wearables: [HAT.urn] }}
      onEquippedChange={onEquippedChange}
    />,
  );
  await userEvent.click(screen.getByTitle("Party Hat"));
  expect(onEquippedChange).toHaveBeenCalledWith([HAT2.urn]);
});

test("preview-before-commit: hovering an unequipped item previews it without persisting", async () => {
  const onEquippedChange = vi.fn();
  render(
    <Backpack
      catalog={[HAT]}
      equipped={BASE}
      onEquippedChange={onEquippedChange}
    />,
  );
  await userEvent.hover(screen.getByTitle("Cool Hat"));
  expect(onEquippedChange).toHaveBeenCalledWith([HAT.urn]);
  expect(send).not.toHaveBeenCalled();

  await userEvent.unhover(screen.getByTitle("Cool Hat"));
  expect(onEquippedChange).toHaveBeenLastCalledWith([]);
  expect(send).not.toHaveBeenCalled();
});
