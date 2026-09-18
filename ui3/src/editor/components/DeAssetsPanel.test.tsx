import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { describe, expect, it } from "vitest";
import { DeCatalogTab } from "./DeAssetsPanel";
import type { DeCatalogItem } from "../types";

const ITEMS: DeCatalogItem[] = [
  { id: "d1", name: "Cyberpunk Sliding Gate", category: "doors", pack: "Smart Items", smart: true },
  { id: "d2", name: "Wooden Door", category: "doors", pack: "Smart Items", smart: true },
  { id: "s1", name: "Park Bench", category: "Seats", pack: "Smart Items", smart: true },
  { id: "b1", name: "Red Button", category: "buttons", pack: "Smart Items", smart: true },
  { id: "p1", name: "Fantasy Door Prop", category: "decorations", pack: "Fantasy" },
];

function names(container: HTMLElement): string[] {
  return [...container.querySelectorAll(".eui-asset .name")].map((el) => el.textContent ?? "");
}

function renderWith(preset: { cat: string; smart: boolean }) {
  return render(
    <DeCatalogTab
      items={ITEMS}
      onPlace={() => {}}
      preset={{ nonce: 1, ...preset, query: "" }}
    />,
  );
}

describe("smart chips filter by category, not name", () => {
  it("shows the smart items of the chip's category, matching the catalog's casing drift, or all smart items with no category", () => {
    const doors = renderWith({ cat: "doors", smart: true });
    const shownDoors = names(doors.container).join(" | ");
    expect(shownDoors).toContain("Cyberpunk Sliding Gate");
    expect(shownDoors).toContain("Wooden Door");
    expect(shownDoors).not.toContain("Fantasy Door Prop");
    expect(shownDoors).not.toContain("Park Bench");
    const select = within(doors.container).getByLabelText("Filter by category") as HTMLSelectElement;
    expect(select.value).toBe("__smart");
    doors.unmount();

    const seats = renderWith({ cat: "seats", smart: true });
    const shownSeats = names(seats.container).join(" | ");
    expect(shownSeats).toContain("Park Bench");
    expect(shownSeats).not.toContain("Wooden Door");
    seats.unmount();

    const all = renderWith({ cat: "", smart: true });
    const shownAll = names(all.container).join(" | ");
    expect(shownAll).toContain("Wooden Door");
    expect(shownAll).toContain("Park Bench");
    expect(shownAll).toContain("Red Button");
    expect(shownAll).not.toContain("Fantasy Door Prop");
  });
});

describe("clicking a catalog card reports what happened", () => {
  const flush = () => act(async () => {
    await Promise.resolve();
    await Promise.resolve();
  });

  it("confirms a placement, surfaces a copy warning, or says why placement failed", async () => {
    const placed: string[] = [];
    const ok = render(
      <DeCatalogTab
        items={ITEMS}
        onPlace={async (a) => {
          placed.push(a.name);
          return { name: a.name, mirrored: true, warning: null };
        }}
      />,
    );
    fireEvent.click(screen.getByText("Wooden Door"));
    expect(placed).toEqual(["Wooden Door"]);
    await flush();
    expect(screen.getByRole("status").textContent).toContain("Placed Wooden Door");
    ok.unmount();

    const warned = render(
      <DeCatalogTab
        items={ITEMS}
        onPlace={async (a) => ({
          name: a.name,
          mirrored: false,
          warning: "its files could not be copied into the project (rpc initAsset failed)",
        })}
      />,
    );
    fireEvent.click(screen.getByText("Wooden Door"));
    await flush();
    expect(screen.getByRole("status").textContent).toContain("could not be copied");
    warned.unmount();

    render(
      <DeCatalogTab
        items={ITEMS}
        onPlace={() => Promise.reject(new Error("the scene is not connected yet"))}
      />,
    );
    fireEvent.click(screen.getByText("Wooden Door"));
    await flush();
    expect(screen.getByRole("alert").textContent).toContain("not connected yet");
  });
});
