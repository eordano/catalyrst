import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";

import FpsMeter, { FPS_GOOD, FPS_WARN, fpsTone } from "./FpsMeter";

describe("fpsTone", () => {
  test("grades on the documented thresholds, and a stalled page is bad", () => {
    expect(fpsTone(FPS_GOOD)).toBe("good");
    expect(fpsTone(FPS_GOOD - 1)).toBe("warn");
    expect(fpsTone(FPS_WARN)).toBe("warn");
    expect(fpsTone(FPS_WARN - 1)).toBe("bad");
    expect(fpsTone(0)).toBe("bad");
  });
});

describe("FpsMeter", () => {
  test("shows the page rate, frame time and engine rate independently, omitting the engine when there is none", () => {
    const { container, rerender } = render(<FpsMeter stats={{ page: 58, engine: 61, ms: 16.7 }} />);
    expect(container.querySelector(".fpsmeter__fps")?.textContent).toBe("58fps");
    expect(screen.getByText("16.7ms")).toBeTruthy();
    expect(container.querySelector(".fpsmeter__engine")?.textContent).toBe("61");

    rerender(<FpsMeter stats={{ page: 34, engine: 59, ms: 29.4 }} />);
    expect(container.querySelector(".fpsmeter__fps")?.className).toContain("is-warn");
    expect(screen.getByText("59")).toBeTruthy();

    rerender(<FpsMeter stats={{ page: 60, engine: null, ms: 16.7 }} />);
    expect(screen.queryByText(/engine/)).toBeNull();
  });
});
