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

test("an empty catalog renders without tiles, and the explicit Unequip button removes an item through the bridge", async () => {
  const empty = render(<Backpack catalog={[]} equipped={BASE} />);
  expect(screen.getByRole("heading", { name: "Backpack" })).toBeInTheDocument();
  expect(document.querySelectorAll("[role=listitem]").length).toBe(0);
  empty.unmount();

  const onEquippedChange = vi.fn();
  render(
    <Backpack
      catalog={[HAT]}
      equipped={{ ...BASE, wearables: [HAT.urn] }}
      onEquippedChange={onEquippedChange}
    />,
  );
  await userEvent.click(screen.getByTitle("Cool Hat"));
  expect(send).not.toHaveBeenCalledWith("SetAvatar", expect.anything());
  await userEvent.click(screen.getByRole("button", { name: "Unequip Cool Hat" }));
  expect(onEquippedChange).toHaveBeenCalledWith([]);
  expect(send).toHaveBeenCalledWith(
    "SetAvatar",
    expect.objectContaining({
      equip: expect.objectContaining({ wearableUrns: [] }),
    }),
  );
});

test("a second hat replaces the first, and hovering previews without persisting", async () => {
  const onReplace = vi.fn();
  const replace = render(
    <Backpack
      catalog={[HAT, HAT2]}
      equipped={{ ...BASE, wearables: [HAT.urn] }}
      onEquippedChange={onReplace}
    />,
  );
  await userEvent.click(screen.getByTitle("Party Hat"));
  await userEvent.click(screen.getByRole("button", { name: "Equip Party Hat" }));
  expect(onReplace).toHaveBeenCalledWith([HAT2.urn]);
  replace.unmount();
  send.mockClear();

  const onPreview = vi.fn();
  render(<Backpack catalog={[HAT]} equipped={BASE} onEquippedChange={onPreview} />);
  await userEvent.hover(screen.getByTitle("Cool Hat"));
  expect(onPreview).toHaveBeenCalledWith([HAT.urn]);
  expect(send).not.toHaveBeenCalled();
  await userEvent.unhover(screen.getByTitle("Cool Hat"));
  expect(onPreview).toHaveBeenLastCalledWith([]);
  expect(send).not.toHaveBeenCalled();
});

test("the MARKETPLACE button links to the marketplace page through the shared story link", () => {
  render(<Backpack catalog={[]} equipped={BASE} />);
  const link = screen.getByRole("button", { name: /Marketplace/ });
  expect(link.getAttribute("data-sb-linkto")).toBe("Explorer/Pages/Marketplace");
});

test("body tiles replace the base and repair body shapes incorrectly saved as clothing", async () => {
  const female = { urn: "urn:decentraland:off-chain:base-avatars:BaseFemale", name: "Female", category: "body_shape" };
  const male = { urn: BASE.bodyShape, name: "Male", category: "body_shape" };
  const onBaseChange = vi.fn();
  const onEquippedChange = vi.fn();
  render(<Backpack catalog={[male, female, HAT]} equipped={{ ...BASE, wearables: [HAT.urn, female.urn] }} onBaseChange={onBaseChange} onEquippedChange={onEquippedChange} />);
  await userEvent.click(screen.getByTitle("Female"));
  await userEvent.click(screen.getByRole("button", { name: "Equip Female" }));
  expect(onBaseChange).toHaveBeenLastCalledWith(expect.objectContaining({ bodyShape: female.urn }));
  expect(onEquippedChange).toHaveBeenLastCalledWith([HAT.urn]);
  expect(send).toHaveBeenLastCalledWith("SetAvatar", expect.objectContaining({
    base: expect.objectContaining({ bodyShapeUrn: female.urn }),
    equip: expect.objectContaining({ wearableUrns: [HAT.urn] }),
  }));
  await userEvent.click(screen.getByTitle("Male"));
  await userEvent.click(screen.getByRole("button", { name: "Equip Male" }));
  expect(onBaseChange).toHaveBeenLastCalledWith(expect.objectContaining({ bodyShape: male.urn }));
  expect(onEquippedChange).toHaveBeenLastCalledWith([HAT.urn]);
});
