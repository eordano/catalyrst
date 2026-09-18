import { act, render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { expect, test, vi } from "vitest";
import type { AvatarStatus } from "../../wearable-preview/avatar";
import { AvatarStage } from "./AvatarPreview";

const preview = vi.hoisted(() => ({ report: (_status: AvatarStatus) => {} }));
vi.mock("../../wearable-preview/WearablePreview", () => ({
  default: ({ onStatus }: { onStatus: (status: AvatarStatus) => void }) => {
    preview.report = onStatus;
    return <div data-testid="preview" />;
  },
}));

test.each(["empty", "error"] as const)("an %s preview offers retry and announces loading until a frame is ready", async status => {
  render(<AvatarStage />);
  expect(screen.getByRole("status")).toHaveTextContent("Loading avatar");
  act(() => preview.report(status));
  expect(screen.queryByTestId("preview")).toBeNull();
  await userEvent.click(screen.getByRole("button", { name: "Retry" }));
  expect(screen.getByTestId("preview")).toBeInTheDocument();
  expect(screen.getByRole("status")).toHaveTextContent("Loading avatar");
  act(() => preview.report("ready"));
  expect(screen.queryByRole("status")).toBeNull();
  expect(screen.getByRole("img", { name: "Avatar preview" })).toBeInTheDocument();
});
