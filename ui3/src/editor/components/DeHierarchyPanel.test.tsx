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

it("exposes all selected rows and extends the selection with modifiers", () => {
  const changes: string[][] = [];
  const { rerender } = render(<DeHierarchyPanel tree={[{ id: "512", name: "Palm" }, { id: "513", name: "Rock" }, { id: "514", name: "Lamp" }]} selectedIds={["512"]} onSelectionChange={ids => changes.push(ids)} />);
  fireEvent.click(screen.getByTitle("Palm"));
  fireEvent.click(screen.getByTitle("Lamp"), { shiftKey: true });
  expect(changes.at(-1)).toEqual(["512", "513", "514"]);
  rerender(<DeHierarchyPanel tree={[{ id: "512", name: "Palm" }, { id: "513", name: "Rock" }, { id: "514", name: "Lamp" }]} selectedIds={["512", "514"]} onSelectionChange={ids => changes.push(ids)} />);
  expect(screen.getByTitle("Palm").getAttribute("aria-selected")).toBe("true");
  expect(screen.getByTitle("Lamp").getAttribute("aria-selected")).toBe("true");
  fireEvent.click(screen.getByTitle("Palm"), { ctrlKey: true });
  expect(changes.at(-1)).toEqual(["514"]);
});

it("routes a drag to the acknowledged move and displays rejected engine writes", async () => {
  const moves: unknown[] = [];
  render(<DeHierarchyPanel tree={TREE} selectedIds={["512"]} onMove={async (...args) => { moves.push(args); throw new Error("The engine disconnected. Reconnect and try again."); }} />);
  const dataTransfer = { getData: () => JSON.stringify(["512"]), types: ["application/x-dcl-entities"] };
  fireEvent.drop(screen.getByTitle("Untitled Scene"), { dataTransfer });
  expect(moves).toEqual([[["512"], "0", "inside"]]);
  expect((await screen.findByRole("alert")).textContent).toContain("engine disconnected");
});

it("waits for clipboard actions and does not intercept typing", async () => {
  let copies = 0;
  render(<DeHierarchyPanel tree={TREE} onCopy={async () => { copies++; }} />);
  fireEvent.keyDown(screen.getByTitle("Palm"), { key: "c", ctrlKey: true });
  expect(copies).toBe(1);
  fireEvent.keyDown(screen.getByPlaceholderText("Search entities\u2026"), { key: "c", ctrlKey: true });
  expect(copies).toBe(1);
});
