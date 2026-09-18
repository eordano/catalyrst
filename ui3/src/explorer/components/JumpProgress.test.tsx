import { act, render, screen } from "@testing-library/react";
import { afterEach, expect, test, vi } from "vitest";
import JumpProgress from "./JumpProgress";
import { EMPTY_TRANSFERS, LOADING_TRANSFER_EVENT, type LoadingTransfers } from "../../overlay/loadingTransferProtocol";

function push(stats: Partial<LoadingTransfers>) {
  act(() => {
    window.__dclLoadingTransfers = { ...EMPTY_TRANSFERS, ...stats };
    window.dispatchEvent(new Event(LOADING_TRANSFER_EVENT));
  });
}
afterEach(() => { delete window.__dclLoadingTransfers; vi.useRealTimers(); });

test("download progress and estimates come from bytes, and unknown totals stay indeterminate", () => {
  vi.useFakeTimers();
  const megabyte = 1024 * 1024;
  render(<JumpProgress />);
  expect(screen.getByRole("progressbar")).not.toHaveAttribute("value");
  push({ receivedBytes: megabyte, started: 2, active: 2, remainingBytes: 3 * megabyte });
  act(() => vi.advanceTimersByTime(1500));
  expect(screen.getByRole("progressbar")).toHaveAttribute("value", "25");
  expect(screen.getByText("1.0 MB received of 4.0 MB requested")).toBeInTheDocument();
  expect(screen.getByText("About 5s for current downloads")).toBeInTheDocument();
  push({ receivedBytes: megabyte, started: 3, active: 3, unknownActive: 1, remainingBytes: 3 * megabyte });
  expect(screen.getByRole("progressbar")).not.toHaveAttribute("value");
  expect(screen.queryByText(/About .*for current downloads/)).toBeNull();
});

test("a new teleport does not inherit previous bytes and completion is separate from scene preparation", () => {
  push({ receivedBytes: 500000, started: 4, completed: 4 });
  render(<JumpProgress />);
  expect(screen.getByText("0 B received")).toBeInTheDocument();
  push({ receivedBytes: 600000, started: 5, completed: 5 });
  expect(screen.getByText("1 download complete")).toBeInTheDocument();
  expect(screen.getByText("Starting the scene")).toBeInTheDocument();
  expect(screen.getByRole("progressbar")).toHaveAttribute("value", "100");
  expect(screen.getByText("Waiting for the scene to be ready\u2026")).toBeInTheDocument();
});
