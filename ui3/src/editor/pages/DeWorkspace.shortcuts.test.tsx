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
  it("Q/W/E/R switch the active tool (aria-pressed)", () => {
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
  });

  it("keys typed into panel inputs never switch tools", () => {
    renderWorkspace();
    const search = screen.getByPlaceholderText("Search entities…");
    search.focus();
    fireEvent.keyDown(search, { key: "e" });
    expect(pressed("Rotate (E)")).toBe("false");
    expect(pressed("Move (W)")).toBe("true");
  });

  it("? opens the cheatsheet, Esc closes it, and tool keys stand down while it is open", () => {
    renderWorkspace();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.getByText(/Keyboard & mouse shortcuts/)).toBeTruthy();

    fireEvent.keyDown(window, { key: "e" });
    expect(pressed("Rotate (E)")).toBe("false");

    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByText(/Keyboard & mouse shortcuts/)).toBeNull();

    fireEvent.keyDown(window, { key: "?" });
    expect(screen.getByText(/Keyboard & mouse shortcuts/)).toBeTruthy();
    fireEvent.keyDown(window, { key: "?" });
    expect(screen.queryByText(/Keyboard & mouse shortcuts/)).toBeNull();
  });

  it("the cheatsheet documents the combined scheme (mouse + views + F5)", () => {
    renderWorkspace();
    fireEvent.keyDown(window, { key: "?" });
    for (const label of ["Orbit", "Focus selection", "Toggle fly camera"]) {
      expect(screen.getByText(label)).toBeTruthy();
    }
    expect(screen.getAllByText(/F5/).length).toBeGreaterThan(0);
  });

  it("Undo/Redo toolbar buttons stay disabled with an empty history", () => {
    renderWorkspace();
    expect((screen.getByTitle("Undo") as HTMLButtonElement).disabled).toBe(true);
    expect((screen.getByTitle("Redo") as HTMLButtonElement).disabled).toBe(true);
  });
});
