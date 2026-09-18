import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test } from "vitest";
import CoverImage from "./CoverImage";

test("failed thumbnails fall back to the original once, then show an accessible placeholder", () => {
  const view = render(<CoverImage src="/thumb" fallbackSrc="/original" alt="Genesis Plaza" />);
  fireEvent.error(screen.getByRole("img"));
  expect(screen.getByRole("img")).toHaveAttribute("src", "/original");
  fireEvent.error(screen.getByRole("img"));
  expect(screen.getByRole("img", { name: "Genesis Plaza" })).not.toHaveAttribute("src");
  view.rerender(<CoverImage src="/different" alt="Another place" />);
  fireEvent.load(screen.getByRole("img"));
  expect(screen.getByRole("img", { name: "Another place" })).toHaveAttribute("src", "/different");
});
