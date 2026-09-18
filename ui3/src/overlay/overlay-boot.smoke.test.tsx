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

test("overlay boots to the guest/login landing, deferring the engine", async () => {
  render(<Composition />);

  const guest = await screen.findByRole("button", { name: "Play as a guest" });
  expect(screen.getByRole("button", { name: "Login or sign up" })).toBeTruthy();
  expect(screen.queryByLabelText("Main menu")).toBeNull();

  expect(window.dclDeferStart).toBe(true);

  await userEvent.click(guest);
  await userEvent.click(screen.getByRole("checkbox"));
  await userEvent.click(screen.getByRole("button", { name: "Let\u2019s go" }));
  expect(screen.queryByText("Pick Your Name")).toBeNull();

  expect(screen.queryByText("Skip to Genesis Plaza")).toBeNull();
  expect(await screen.findByRole("main", { name: "Decentraland lobby" })).toBeInTheDocument();
});
