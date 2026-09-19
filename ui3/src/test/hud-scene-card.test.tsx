import { afterEach, beforeEach, expect, test } from "vitest";
import { screen, waitFor } from "@testing-library/react";
import { renderHud } from "./harness";
import { SIDEBAR_DESIGN_STORAGE } from "../data/sidebarDesignFlag";

beforeEach(() => localStorage.setItem(SIDEBAR_DESIGN_STORAGE, "1"));
afterEach(() => localStorage.removeItem(SIDEBAR_DESIGN_STORAGE));

test("location saves by parcel ID despite a changed title and reloads the signed-in favorite", async () => {
  const { bridge, user } = renderHud();
  bridge.pushIdentity({ isGuest: false });
  bridge.pushScene({ title: "New scene title", coords: "2,12" });
  await user.click(screen.getByRole("button", { name: "Location" }));
  const reads = () => bridge.sentOf("SignedFetch").filter(p => p.method === "GET" && p.url.includes("/api/places?"));
  await waitFor(() => expect(reads().length).toBeGreaterThan(0));
  const row = { id: "place-id", title: "Old catalog title", base_position: "2,12", positions: ["2,12"], categories: [], user_visits: 0, favorites: 0, likes: 0, highlighted: false, world: false };
  const reply = (id: string, favorite: boolean) => bridge.push({ kind: "signedFetchResult", id, status: 200, body: JSON.stringify({ ok: true, total: 1, data: [{ ...row, user_favorite: favorite }] }) });
  reply(reads().at(-1)!.id, false);
  await user.click(screen.getByRole("button", { name: "Scene options" }));
  const save = screen.getByRole("menuitem", { name: "Save place to favorites" });
  await waitFor(() => expect(save).toBeEnabled());
  await user.click(save);
  const write = bridge.expectSent("SignedFetch", p => p.method === "PATCH" && p.url.endsWith("/api/places/place-id/favorites"));
  expect(JSON.parse(write.body!)).toEqual({ favorites: true });
  bridge.push({ kind: "signedFetchResult", id: write.id, status: 204, body: "" });
  await user.click(screen.getByRole("button", { name: "Scene options" }));
  expect(await screen.findByRole("menuitem", { name: "Remove place from favorites" })).toBeInTheDocument();
  const firstRead = reads()[0]!.id;
  await waitFor(() => expect(reads().at(-1)!.id).not.toBe(firstRead));
  reply(reads().at(-1)!.id, true);
  await user.click(screen.getByRole("button", { name: "Location" }));
  await user.click(screen.getByRole("button", { name: "Location" }));
  await user.click(screen.getByRole("button", { name: "Scene options" }));
  expect(await screen.findByRole("menuitem", { name: "Remove place from favorites" })).toBeInTheDocument();
});
