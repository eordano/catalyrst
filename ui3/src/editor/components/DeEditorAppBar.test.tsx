import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import DeEditorAppBar from "./DeEditorAppBar";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

const options = [{ id: "scene", label: "Publish scene" }, { id: "world", label: "Publish to World" }];

describe("DeEditorAppBar", () => {
  it("prevents preview and publishing before the engine is ready and during a save", () => {
    const publish = vi.fn();
    const props = { title: "Untitled Scene", viewportSrc: "/play/?systemScene=editor", publishOptions: options, onPublish: publish };
    const { rerender } = render(<DeEditorAppBar {...props} engine="connecting" />);
    for (const engine of ["connecting", "offline"] as const) {
      rerender(<DeEditorAppBar {...props} engine={engine} />);
      for (const name of ["Preview", "Publish", "Publish options"]) {
        const button = screen.getByRole("button", { name }) as HTMLButtonElement;
        expect(button.disabled).toBe(true);
        fireEvent.click(button);
      }
    }
    expect(publish).not.toHaveBeenCalled();
    rerender(<DeEditorAppBar {...props} engine="online" />);
    fireEvent.click(screen.getByRole("button", { name: "Publish" }));
    expect(publish).toHaveBeenCalledOnce();
    rerender(<DeEditorAppBar {...props} engine="online" busy />);
    expect((screen.getByRole("button", { name: "Publish" }) as HTMLButtonElement).disabled).toBe(true);
  });

  it("preserves guarded back navigation and opens the player preview without editor parameters", () => {
    const back = vi.fn();
    const open = vi.spyOn(window, "open").mockReturnValue(null);
    render(<DeEditorAppBar title="My scene" viewportSrc="https://catalyst.example.com/play/?systemScene=editor&editorUi=1&realm=project" engine="online" onExit={back} />);
    fireEvent.click(screen.getByRole("button", { name: "Back to Creator Hub" }));
    expect(back).toHaveBeenCalledOnce();
    fireEvent.click(screen.getByRole("button", { name: "Preview" }));
    expect(open).toHaveBeenCalledWith("https://catalyst.example.com/play/?realm=project", "_blank", "noopener,noreferrer");
    expect(screen.queryByRole("button", { name: "Preview options" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Exit" })).toBeNull();
  });

  it("renames on commit, rejects empty names, and cancels on Escape", () => {
    const rename = vi.fn();
    render(<DeEditorAppBar title="My scene" onRename={rename} />);
    const input = screen.getByRole("textbox", { name: "Scene title" }) as HTMLInputElement;
    fireEvent.change(input, { target: { value: "  Moonlit Garden  " } });
    fireEvent.blur(input);
    expect(rename).toHaveBeenCalledWith("Moonlit Garden");
    expect(input.value).toBe("My scene");
    rename.mockClear();
    fireEvent.change(input, { target: { value: " " } });
    fireEvent.blur(input);
    expect(input.value).toBe("My scene");
    fireEvent.change(input, { target: { value: "Not saved" } });
    fireEvent.keyDown(input, { key: "Escape" });
    fireEvent.blur(input);
    expect(rename).not.toHaveBeenCalled();
    expect(input.value).toBe("My scene");
  });

  it("dismisses publish options on outside click, Escape, toggle and window blur", () => {
    render(<div><p data-testid="outside">Elsewhere</p><DeEditorAppBar title="My scene" publishOptions={options} onPublish={() => {}} /></div>);
    const open = () => fireEvent.click(screen.getByLabelText("Publish options"));
    open();
    fireEvent.mouseDown(screen.getByText("Publish scene"));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByRole("menu")).toBeNull();
    open(); fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();
    open(); open();
    expect(screen.queryByRole("menu")).toBeNull();
    open(); fireEvent.blur(window);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
