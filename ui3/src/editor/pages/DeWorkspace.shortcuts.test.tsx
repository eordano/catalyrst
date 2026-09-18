import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import DeWorkspace from "./DeWorkspace";

afterEach(cleanup);

const pressed = (title: string) =>
  screen.getByTitle(title).getAttribute("aria-pressed");

function renderWorkspace() {
  return render(<DeWorkspace title="Shortcut Proof" tree={[]} inspector={{}} />);
}

describe("DeWorkspace discrete shortcuts", () => {
  it("Q/W/E/R switch the active tool, keys typed into panel inputs never do, and Undo/Redo stay disabled with an empty history", () => {
    renderWorkspace();
    expect(pressed("Move (W)")).toBe("true");

    fireEvent.keyDown(window, { key: "e" });
    expect(pressed("Rotate (E)")).toBe("true");
    expect(pressed("Move (W)")).toBe("false");

    fireEvent.keyDown(window, { key: "r" });
    expect(pressed("Scale (R)")).toBe("true");

    fireEvent.keyDown(window, { key: "q" });
    expect(pressed("Select (Q)")).toBe("true");

    fireEvent.keyDown(window, { key: "w" });
    expect(pressed("Move (W)")).toBe("true");

    const search = screen.getByPlaceholderText("Search entities\u{2026}");
    search.focus();
    fireEvent.keyDown(search, { key: "e" });
    expect(pressed("Rotate (E)")).toBe("false");
    expect(pressed("Move (W)")).toBe("true");

    for (const label of ["Undo", "Redo"]) {
      const btn = screen.getByLabelText(label) as HTMLButtonElement;
      expect(btn.disabled).toBe(true);
      expect(btn.title.length).toBeGreaterThan(label.length);
    }
  });

  it("? toggles the cheatsheet, Esc closes it, and tool keys stand down while it is open", () => {
    renderWorkspace();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.getByText(/Keyboard & mouse shortcuts/)).toBeTruthy();
    expect(screen.getAllByText(/F5/).length).toBeGreaterThan(0);

    fireEvent.keyDown(window, { key: "e" });
    expect(pressed("Rotate (E)")).toBe("false");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByText(/Keyboard & mouse shortcuts/)).toBeNull();

    fireEvent.keyDown(window, { key: "?" });
    expect(screen.getByText(/Keyboard & mouse shortcuts/)).toBeTruthy();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.queryByText(/Keyboard & mouse shortcuts/)).toBeNull();
  });
});


it.each(["scene", "assets"] as const)("returns to the hierarchy from assets entered via %s", (left) => {
  render(<DeWorkspace title="Asset navigation" left={left} tree={[]} inspector={{}} />);
  if (left === "scene") fireEvent.click(screen.getByRole("button", { name: "Browse asset catalog" }));
  fireEvent.click(screen.getByRole("button", { name: "Back to scene" }));
  expect(screen.getByPlaceholderText("Search entities\u{2026}")).toBeTruthy();
  fireEvent.click(screen.getByRole("button", { name: "Browse asset catalog" }));
  expect(screen.getByRole("button", { name: "Back to scene" })).toBeTruthy();
});
