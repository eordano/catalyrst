import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import { DeHierarchyPanel } from "./DeHierarchyPanel";

afterEach(cleanup);

const TREE = [
  { id: "0", name: "Untitled Scene", expanded: true, children: [{ id: "512", name: "Palm" }] },
];

describe("DeHierarchyPanel focus", () => {
  it("single click selects, double click focuses the entity, and double click on the root focuses the whole scene", () => {
    const selected: string[] = [];
    const focused: string[] = [];
    render(
      <DeHierarchyPanel
        tree={TREE}
        title="Hierarchy"
        live
        onSelect={(id) => selected.push(String(id))}
        onFocus={(id) => focused.push(String(id))}
      />,
    );
    const row = screen.getByTitle("Palm");
    fireEvent.click(row);
    expect(selected).toEqual(["512"]);
    expect(focused).toEqual([]);
    fireEvent.doubleClick(row);
    expect(focused).toEqual(["512"]);
    fireEvent.doubleClick(screen.getByTitle("Untitled Scene"));
    expect(focused).toEqual(["512", "0"]);
  });
});
