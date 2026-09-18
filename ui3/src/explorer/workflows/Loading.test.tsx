import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";

import Loading from "./Loading";
import { LOADING_TIPS, TIP_ROTATION_MS } from "./loadingTips";

const TIP_INDEX_STORAGE_KEY = "dcl-loading-tip-index";

const advance = (ms: number) => act(() => vi.advanceTimersByTime(ms));

function activeTitle(): string | null {
  return (
    document
      .querySelector(".loading__pane.is-active .loading__title")
      ?.textContent ?? null
  );
}

beforeEach(() => {
  vi.useFakeTimers();
  localStorage.removeItem(TIP_INDEX_STORAGE_KEY);
});
afterEach(() => {
  vi.useRealTimers();
});

describe("Loading tip carousel", () => {
  test("renders the progress header, the first tip and one dot per tip, then auto-rotates every 10s and wraps", () => {
    render(<Loading progress={40} initialTip={0} />);
    expect(screen.getByText(/40%/)).toBeInTheDocument();
    expect(activeTitle()).toBe("Take a Shot");
    expect(screen.getAllByRole("button", { name: /^Tip / })).toHaveLength(
      LOADING_TIPS.length,
    );
    advance(TIP_ROTATION_MS);
    expect(activeTitle()).toBe("Show Up");
    advance(TIP_ROTATION_MS * 8);
    expect(activeTitle()).toBe("Hang Out");
    advance(TIP_ROTATION_MS);
    expect(activeTitle()).toBe("Take a Shot");
  });

  test("breadcrumbs and arrows navigate with wrap-around, persist the index, and reset the auto-rotate timer", () => {
    render(<Loading progress={40} initialTip={0} />);
    advance(TIP_ROTATION_MS / 2);
    fireEvent.click(screen.getByRole("button", { name: "Tip 6" }));
    expect(activeTitle()).toBe("Your Look");
    expect(localStorage.getItem(TIP_INDEX_STORAGE_KEY)).toBe("5");
    fireEvent.click(screen.getByRole("button", { name: "Previous tip" }));
    expect(activeTitle()).toBe("Say Hi!");
    fireEvent.click(screen.getByRole("button", { name: "Previous tip" }));
    expect(activeTitle()).toBe("Your Presence");
    advance(TIP_ROTATION_MS - 1);
    expect(activeTitle()).toBe("Your Presence");
    advance(1);
    expect(activeTitle()).toBe("Say Hi!");

    fireEvent.click(screen.getByRole("button", { name: "Tip 1" }));
    expect(activeTitle()).toBe("Take a Shot");
    fireEvent.click(screen.getByRole("button", { name: "Previous tip" }));
    expect(activeTitle()).toBe("Hang Out");
    fireEvent.click(screen.getByRole("button", { name: "Next tip" }));
    expect(activeTitle()).toBe("Take a Shot");
    fireEvent.click(screen.getByRole("button", { name: "Next tip" }));
    expect(activeTitle()).toBe("Show Up");
    expect(localStorage.getItem(TIP_INDEX_STORAGE_KEY)).toBe("1");
  });

  test("resumes from the persisted tip index without an initialTip, and a garbage index falls back to the first tip", () => {
    localStorage.setItem(TIP_INDEX_STORAGE_KEY, "9");
    const resumed = render(<Loading progress={40} />);
    expect(activeTitle()).toBe("Hang Out");
    resumed.unmount();

    localStorage.setItem(TIP_INDEX_STORAGE_KEY, "not-a-number");
    render(<Loading progress={40} />);
    expect(activeTitle()).toBe("Take a Shot");
  });
});
