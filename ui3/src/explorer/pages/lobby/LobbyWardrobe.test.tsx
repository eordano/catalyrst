import { expect, test, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import LobbyWardrobe, { type LobbyLook } from "./LobbyWardrobe";
import { relativeStart } from "./LobbyCards";

test("equipping replaces only the same wearable category and preserves emotes and colors", async () => {
  const look: LobbyLook = { name: "Guest", bodyShape: "male", wearables: ["shirt", "shoes"], emotes: ["wave"], skinColor: "#123456", hairColor: "#654321", eyeColor: "#abcdef" };
  const change = vi.fn();
  render(<LobbyWardrobe look={look} catalog={[
    { urn: "shirt", name: "Shirt", category: "upper_body" },
    { urn: "jacket", name: "Jacket", category: "upper_body" },
    { urn: "shoes", name: "Shoes", category: "feet" },
  ]} pending={false} error={false} onChange={change} onSave={vi.fn()} onCancel={vi.fn()} onRetry={vi.fn()} />);
  await userEvent.click(screen.getByRole("button", { name: "Equip Jacket" }));
  expect(change).toHaveBeenCalledWith({ ...look, wearables: ["shoes", "jacket"] });
});

test("upcoming times handle imminent, hourly and daily starts", () => {
  expect(relativeStart(0, 1)).toBe("Starting now");
  expect(relativeStart(59000, 0)).toBe("In 1 minute");
  expect(relativeStart(90 * 60000, 0)).toBe("In 2 hours");
  expect(relativeStart(25 * 3600000, 0)).toBe("In 2 days");
});
