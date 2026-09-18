import { fireEvent, render, screen } from "@testing-library/react";
import { expect, test, vi } from "vitest";
import Passport from "./Passport";
import { sendBridge } from "../../overlay/bridge";
vi.mock("../../overlay/bridge", () => ({ sendBridge: vi.fn(), getBridge: () => null }));

test("only the own profile offers avatar editing", () => {
  const own = render(<Passport isSelf identity={{ name: "Brown" }} />);
  expect(screen.getByRole("button", { name: /edit avatar/i })).toBeInTheDocument();
  own.unmount();
  render(<Passport isSelf={false} identity={{ name: "Lu" }} />);
  expect(screen.queryByRole("button", { name: /edit avatar/i })).toBeNull();
});

test("name editing stays open on blur and saves explicitly through the bridge", () => {
  render(<Passport identity={{ name: "Brown" }} base={{ bodyShape: "body", skinColor: "#123456" }} />);
  fireEvent.click(screen.getByRole("button", { name: "Edit name" }));
  const input = screen.getByRole("textbox", { name: "Edit name" });
  fireEvent.change(input, { target: { value: "NewName" } });
  fireEvent.blur(input);
  expect(input).toBeInTheDocument();
  fireEvent.click(screen.getByRole("button", { name: "Save name" }));
  expect(sendBridge).toHaveBeenCalledWith("SetAvatar", expect.objectContaining({ base: expect.objectContaining({ name: "NewName", bodyShapeUrn: "body" }) }));
  expect(screen.queryByRole("textbox")).toBeNull();
});

test("overview excludes badges and the badges tab excludes overview sections", () => {
  render(<Passport badges={[{id:"badge",name:"Explorer"}]} about="About this explorer" />);
  expect(screen.getByText("About this explorer")).toBeInTheDocument();
  expect(screen.queryByRole("heading", { name: "Badges" })).toBeNull();
  fireEvent.click(screen.getByRole("tab", { name: "Badges" }));
  expect(screen.getByRole("heading", { name: "Badges" })).toBeInTheDocument();
  expect(screen.queryByText("About this explorer")).toBeNull();
});

test("dismisses only a backdrop click and respects the selected badges tab", () => {
  const onClose = vi.fn();
  const { container } = render(<Passport tab="badges" onClose={onClose} />);
  expect(screen.getByRole("tab", { name: "Badges" })).toHaveAttribute("aria-selected", "true");
  fireEvent.click(container.querySelector(".ps")!);
  expect(onClose).not.toHaveBeenCalled();
  fireEvent.click(container.querySelector(".ps__backdrop")!);
  expect(onClose).toHaveBeenCalledTimes(1);
});
