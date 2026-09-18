import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import MobileActionCluster from "./MobileActionCluster";
import MobileSheet from "./MobileSheet";
import MobileTabBar from "./MobileTabBar";
import MobileTopBar from "./MobileTopBar";

describe("MobileActionCluster", () => {
  it("presses and releases the held action once per gesture, ignores a second pointer, releases on cancel and unmount, and renders nothing when every action is hidden", () => {
    const onPress = vi.fn();
    const onRelease = vi.fn();
    const view = render(<MobileActionCluster onActionPress={onPress} onActionRelease={onRelease} />);
    const jump = screen.getByRole("button", { name: "Jump" });
    fireEvent.pointerDown(jump, { pointerId: 1 });
    expect(onPress).toHaveBeenCalledWith("jump");
    fireEvent.pointerDown(jump, { pointerId: 2 });
    expect(onPress).toHaveBeenCalledTimes(1);
    fireEvent.pointerUp(jump, { pointerId: 1 });
    expect(onRelease).toHaveBeenCalledWith("jump");
    expect(onRelease).toHaveBeenCalledTimes(1);

    fireEvent.pointerDown(jump, { pointerId: 1 });
    fireEvent.pointerCancel(jump, { pointerId: 1 });
    expect(onRelease).toHaveBeenCalledTimes(2);

    fireEvent.pointerDown(jump, { pointerId: 2 });
    view.unmount();
    expect(onRelease).toHaveBeenCalledTimes(3);
    expect(onRelease).toHaveBeenLastCalledWith("jump");

    const { container } = render(
      <MobileActionCluster actions={[{ id: "jump", label: "Jump", glyph: "jump", hidden: true }]} />,
    );
    expect(container.firstChild).toBeNull();
  });
});

describe("MobileTabBar", () => {
  it("unmounts in landscape by default but keeps the pill variant when asked", () => {
    const { container, rerender } = render(<MobileTabBar orientation="landscape" />);
    expect(container.firstChild).toBeNull();
    rerender(<MobileTabBar orientation="landscape" landscape="pill" active="map" />);
    expect(screen.getByRole("button", { name: "Map" })).toHaveAttribute("aria-current", "page");
  });
});

describe("MobileSheet", () => {
  it("renders nothing while closed and snaps up past the drag threshold once open", () => {
    const onSnapChange = vi.fn();
    const { container, rerender } = render(<MobileSheet title="Place details" />);
    expect(container.firstChild).toBeNull();
    rerender(
      <MobileSheet open title="Place details" defaultSnap="half" onSnapChange={onSnapChange} />,
    );
    const grab = document.querySelector(".msh__grab");
    expect(grab).not.toBeNull();
    fireEvent.pointerDown(grab as Element, { pointerId: 1, clientY: 400 });
    fireEvent.pointerMove(grab as Element, { pointerId: 1, clientY: 300 });
    fireEvent.pointerUp(grab as Element, { pointerId: 1, clientY: 300 });
    expect(onSnapChange).toHaveBeenCalledWith("full");
  });

  it("closes from peek when dragged down, and uses the drawer arrangement in landscape without a drag handle", () => {
    const onClose = vi.fn();
    const { rerender } = render(
      <MobileSheet open title="Place details" defaultSnap="peek" onClose={onClose} />,
    );
    const grab = document.querySelector(".msh__grab") as Element;
    fireEvent.pointerDown(grab, { pointerId: 1, clientY: 200 });
    fireEvent.pointerUp(grab, { pointerId: 1, clientY: 300 });
    expect(onClose).toHaveBeenCalled();

    rerender(<MobileSheet open orientation="landscape" title="Place details" />);
    expect(document.querySelector(".msh__grab")).toBeNull();
    expect(screen.getByRole("dialog", { name: "Place details" })).toBeInTheDocument();
  });
});

describe("MobileTopBar", () => {
  it("states the empty location honestly", () => {
    render(<MobileTopBar orientation="portrait" />);
    expect(screen.getByText("Unknown parcel")).toBeInTheDocument();
    expect(screen.getByText("no coordinates")).toBeInTheDocument();
  });
});
