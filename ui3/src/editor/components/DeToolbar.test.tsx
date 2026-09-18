import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { DeToolbar } from "./DeToolbar";

afterEach(cleanup);

describe("DeToolbar camera menu", () => {
  it("closes on outside click, window blur and Escape, but stays open for clicks inside", () => {
    render(
      <div>
        <p data-testid="outside">elsewhere</p>
        <DeToolbar live onCamMode={vi.fn()} />
      </div>,
    );
    const open = () => {
      fireEvent.click(screen.getByLabelText("Camera mode"));
      expect(screen.getByText("Free fly")).toBeTruthy();
    };

    open();
    fireEvent.mouseDown(screen.getByText("Free fly"));
    expect(screen.getByText("Free fly")).toBeTruthy();
    fireEvent.mouseDown(screen.getByTestId("outside"));
    expect(screen.queryByText("Free fly")).toBeNull();

    open();
    fireEvent.blur(window);
    expect(screen.queryByText("Free fly")).toBeNull();

    open();
    fireEvent.keyDown(document, { key: "Escape" });
    expect(screen.queryByText("Free fly")).toBeNull();
  });
});
