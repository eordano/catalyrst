import { test, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { RouterProvider } from "react-router";

import { queryClient } from "../app/queryClient";
import { router } from "../app/router";
import BootGate from "../app/BootGate";

function Composition() {
  return (
    <QueryClientProvider client={queryClient}>
      <BootGate>
        <RouterProvider router={router} />
      </BootGate>
    </QueryClientProvider>
  );
}

test("overlay boots to the lobby with Continue as guest, deferring the engine", async () => {
  render(<Composition />);

  const jump = await screen.findByText("Continue as guest");
  expect(jump).toBeTruthy();
  expect(screen.queryByLabelText("Main menu")).toBeNull();

  expect(window.dclDeferStart).toBe(true);

  await userEvent.click(screen.getByRole("checkbox"));
  await userEvent.click(jump.closest("button") ?? jump);
  expect(screen.queryByText("Continue as guest")).toBeNull();

  // Continue as guest advances to the destination picker; skipping it hands off to the loading gate.
  const skip = await screen.findByText("Skip to Genesis Plaza");
  await userEvent.click(skip.closest("button") ?? skip);
  expect(document.querySelector(".boot")).toBeTruthy();
});
