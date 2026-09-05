// Pins what the Interact tab's chips mean: {cat:"doors", smart:true} is the
// doors SHELF (smart items of that category, case-insensitively -- the catalog
// holds "Seats" next to "doors"), never a substring search. The chips shipped
// as query:"door" once, which surfaced anything door-NAMED and missed a smart
// item named without its category word.
import { render, screen } from "@testing-library/react";
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
  it("the doors chip shows every smart door and nothing else", () => {
    const { container } = renderWith({ cat: "doors", smart: true });
    const shown = names(container).join(" | ");
    expect(shown).toContain("Cyberpunk Sliding Gate");
    expect(shown).toContain("Wooden Door");
    expect(shown).not.toContain("Fantasy Door Prop");
    expect(shown).not.toContain("Park Bench");
  });

  it("matches the catalog's own casing drift (Seats)", () => {
    const { container } = renderWith({ cat: "seats", smart: true });
    const shown = names(container).join(" | ");
    expect(shown).toContain("Park Bench");
    expect(shown).not.toContain("Wooden Door");
  });

  it("smart alone still means all smart items", () => {
    const { container } = renderWith({ cat: "", smart: true });
    const shown = names(container).join(" | ");
    expect(shown).toContain("Wooden Door");
    expect(shown).toContain("Park Bench");
    expect(shown).toContain("Red Button");
    expect(shown).not.toContain("Fantasy Door Prop");
  });

  it("the category select reads Smart Items while a chip filter is active", () => {
    renderWith({ cat: "doors", smart: true });
    const select = screen.getByLabelText("Filter by category") as HTMLSelectElement;
    expect(select.value).toBe("__smart");
  });
});
