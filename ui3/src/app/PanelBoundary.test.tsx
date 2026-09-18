import { lazy } from "react";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import PanelBoundary from "./PanelBoundary";

afterEach(() => { cleanup(); vi.restoreAllMocks(); });

it("keeps a named, dismissible loading state until a lazy panel is ready", async () => {
  let resolve!: (module: { default: () => React.ReactNode }) => void;
  const Panel = lazy(() => new Promise<{ default: () => React.ReactNode }>(done => { resolve = done; }));
  const close = vi.fn();
  render(<PanelBoundary label="camera" onClose={close} standalone><Panel /></PanelBoundary>);
  expect(screen.getByRole("status")).toHaveTextContent("Opening camera");
  expect(screen.getByRole("status")).toHaveAttribute("aria-busy", "true");
  fireEvent.click(screen.getByRole("button", { name: "Close" }));
  expect(close).toHaveBeenCalledOnce();
  await act(async () => resolve({ default: () => <p>Camera ready</p> }));
  expect(screen.getByText("Camera ready")).toBeVisible();
  expect(screen.queryByRole("status")).toBeNull();
});

it("contains a failed lazy import and keeps recovery and surrounding content available", async () => {
  vi.spyOn(console, "error").mockImplementation(() => {});
  let reject!: (error: Error) => void;
  const Panel = lazy(() => new Promise<{ default: () => null }>((_, fail) => { reject = fail; }));
  render(<><button>World controls</button><PanelBoundary label="gallery" onClose={() => {}}><Panel /></PanelBoundary></>);
  await act(async () => reject(new Error("chunk offline")));
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn't open gallery");
  expect(screen.getByRole("button", { name: "Reload" })).toBeVisible();
  expect(screen.getByRole("button", { name: "World controls" })).toBeVisible();
  expect(screen.queryByRole("status")).toBeNull();
});
