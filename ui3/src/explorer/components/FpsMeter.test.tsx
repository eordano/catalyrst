import { describe, expect, test } from "vitest";
import { render, screen } from "@testing-library/react";

import FpsMeter, { FPS_GOOD, FPS_WARN, fpsTone } from "./FpsMeter";

const meter = (c: HTMLElement) => c.querySelector(".fpsmeter__fps")?.className ?? "";

describe("fpsTone", () => {
  test("grades on the documented thresholds", () => {
    expect(fpsTone(FPS_GOOD)).toBe("good");
    expect(fpsTone(FPS_GOOD - 1)).toBe("warn");
    expect(fpsTone(FPS_WARN)).toBe("warn");
    expect(fpsTone(FPS_WARN - 1)).toBe("bad");
  });

  test("a stalled page is bad, not good", () => {
    expect(fpsTone(0)).toBe("bad");
  });
});

describe("FpsMeter", () => {
  test("shows the page rate and frame time", () => {
    const { container } = render(<FpsMeter stats={{ page: 58, engine: 61, ms: 16.7 }} />);
    expect(container.querySelector(".fpsmeter__fps")?.textContent).toBe("58fps");
    expect(screen.getByText("16.7ms")).toBeTruthy();
    expect(container.querySelector(".fpsmeter__engine")?.textContent).toBe("61");
  });

  test("carries the tone as a class", () => {
    const { container } = render(<FpsMeter stats={{ page: 60, engine: 60, ms: 16 }} />);
    expect(meter(container)).toContain("is-good");
  });

  // the whole point of showing both: a healthy engine behind a stalling page means
  // the HUD is eating the frame budget, not the renderer
  test("reports page and engine independently", () => {
    const { container } = render(<FpsMeter stats={{ page: 34, engine: 59, ms: 29.4 }} />);
    expect(meter(container)).toContain("is-warn");
    expect(screen.getByText("59")).toBeTruthy();
  });

  test("omits the engine reading when there is no engine", () => {
    render(<FpsMeter stats={{ page: 60, engine: null, ms: 16.7 }} />);
    expect(screen.queryByText(/engine/)).toBeNull();
  });

  test("is inert to the pointer and hidden from assistive tech", () => {
    const { container } = render(<FpsMeter stats={{ page: 60, engine: 60, ms: 16 }} />);
    expect(container.querySelector(".fpsmeter")?.getAttribute("aria-hidden")).toBe("true");
  });
});
