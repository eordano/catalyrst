import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import DeEditorAppBar from "./DeEditorAppBar";

afterEach(cleanup);

function openPreviewMenu() {
  fireEvent.click(screen.getByLabelText("Preview options"));
  expect(screen.getByRole("menu")).toBeTruthy();
}

describe("DeEditorAppBar menus", () => {
  it("close on outside click, Escape, caret toggle and window blur, but stay open for clicks inside", () => {
    render(
      <div>
        <p data-testid="outside">elsewhere</p>
        <DeEditorAppBar title="Untitled Scene" viewportSrc="https://catalyst.example.com/play?x=1" />
      </div>,
    );
    openPreviewMenu();
    fireEvent.mouseDown(screen.getByText("Preview Options"));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByRole("menu")).toBeNull();

    openPreviewMenu();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByRole("menu")).toBeNull();

    openPreviewMenu();
    fireEvent.click(screen.getByLabelText("Preview options"));
    expect(screen.queryByRole("menu")).toBeNull();

    fireEvent.click(screen.getByLabelText("Publish options"));
    expect(screen.getByRole("menu")).toBeTruthy();
    fireEvent.blur(window);
    expect(screen.queryByRole("menu")).toBeNull();
  });
});
