import { act, fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import Backpack from "./Backpack";

test("outfit loading and failures never expose empty slots or allow overwriting unknown outfits", () => {
  const retry = vi.fn();
  const view = render(<Backpack outfitsLoading />);
  fireEvent.click(screen.getByRole("tab", { name: /outfits/i }));
  expect(screen.getByRole("status")).toHaveTextContent("Loading saved outfits");
  expect(screen.queryByText("Empty Slot")).toBeNull();
  expect(screen.queryByRole("button", { name: /SAVE OUTFIT/ })).toBeNull();
  view.rerender(<Backpack outfitsError onRetryOutfits={retry} />);
  expect(screen.getByRole("alert")).toHaveTextContent("Couldn't load saved outfits");
  fireEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(retry).toHaveBeenCalledOnce();
  view.rerender(<Backpack />);
  expect(screen.getAllByText("Empty Slot")).toHaveLength(5);
});

test("saving waits for confirmation, preserves slots after failure, and can be retried", async () => {
  let reject!: (error: Error) => void;
  const save = vi.fn().mockImplementationOnce(() => new Promise<void>((_, fail) => { reject = fail; })).mockResolvedValue(undefined);
  render(<Backpack onOutfitsChange={save} />);
  fireEvent.click(screen.getByRole("tab", { name: /outfits/i }));
  const button = screen.getByRole("button", { name: /SAVE OUTFIT/ });
  fireEvent.click(button);
  expect(button).toBeDisabled();
  expect(screen.getByRole("status")).toHaveTextContent("Saving outfits");
  expect(screen.getAllByText("Empty Slot")).toHaveLength(5);
  await act(async () => reject(new Error("Service unavailable. Please try again.")));
  expect(screen.getByRole("alert")).toHaveTextContent("Service unavailable");
  expect(screen.getAllByText("Empty Slot")).toHaveLength(5);
  await act(async () => fireEvent.click(button));
  expect(save).toHaveBeenCalledTimes(2);
  expect(screen.getAllByText("Empty Slot")).toHaveLength(4);
  expect(screen.queryByRole("alert")).toBeNull();
});
