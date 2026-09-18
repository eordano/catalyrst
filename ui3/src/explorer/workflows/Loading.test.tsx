import { afterEach, beforeEach, describe, expect, test, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";

import Loading from "./Loading";
import { LOADING_TIPS, TIP_ROTATION_MS } from "./loadingTips";

const TIP_INDEX_STORAGE_KEY = "dcl-loading-tip-index";
const LAST = LOADING_TIPS.length - 1;
const titleAt = (i: number) => LOADING_TIPS[i]!.title;

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
    expect(activeTitle()).toBe(titleAt(0));
    expect(screen.getAllByRole("button", { name: /^Tip / })).toHaveLength(
      LOADING_TIPS.length,
    );
    advance(TIP_ROTATION_MS);
    expect(activeTitle()).toBe(titleAt(1));
    advance(TIP_ROTATION_MS * (LAST - 1));
    expect(activeTitle()).toBe(titleAt(LAST));
    advance(TIP_ROTATION_MS);
    expect(activeTitle()).toBe(titleAt(0));
  });

  test("breadcrumbs and arrows navigate with wrap-around, persist the index, and reset the auto-rotate timer", () => {
    render(<Loading progress={40} initialTip={0} />);
    advance(TIP_ROTATION_MS / 2);
    fireEvent.click(screen.getByRole("button", { name: "Tip 6" }));
    expect(activeTitle()).toBe(titleAt(5));
    expect(localStorage.getItem(TIP_INDEX_STORAGE_KEY)).toBe("5");
    fireEvent.click(screen.getByRole("button", { name: "Previous tip" }));
    expect(activeTitle()).toBe(titleAt(4));
    fireEvent.click(screen.getByRole("button", { name: "Previous tip" }));
    expect(activeTitle()).toBe(titleAt(3));
    advance(TIP_ROTATION_MS - 1);
    expect(activeTitle()).toBe(titleAt(3));
    advance(1);
    expect(activeTitle()).toBe(titleAt(4));

    fireEvent.click(screen.getByRole("button", { name: "Tip 1" }));
    expect(activeTitle()).toBe(titleAt(0));
    fireEvent.click(screen.getByRole("button", { name: "Previous tip" }));
    expect(activeTitle()).toBe(titleAt(LAST));
    fireEvent.click(screen.getByRole("button", { name: "Next tip" }));
    expect(activeTitle()).toBe(titleAt(0));
    fireEvent.click(screen.getByRole("button", { name: "Next tip" }));
    expect(activeTitle()).toBe(titleAt(1));
    expect(localStorage.getItem(TIP_INDEX_STORAGE_KEY)).toBe("1");
  });

  test("resumes from the persisted tip index without an initialTip, and a garbage index falls back to the first tip", () => {
    localStorage.setItem(TIP_INDEX_STORAGE_KEY, String(LAST));
    const resumed = render(<Loading progress={40} />);
    expect(activeTitle()).toBe(titleAt(LAST));
    resumed.unmount();

    localStorage.setItem(TIP_INDEX_STORAGE_KEY, "not-a-number");
    render(<Loading progress={40} />);
    expect(activeTitle()).toBe(titleAt(0));
  });
});
