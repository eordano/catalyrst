import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import MobileActionCluster from "./MobileActionCluster";
import MobileHudFrame from "./MobileHudFrame";
import MobileSheet from "./MobileSheet";
import MobileTabBar from "./MobileTabBar";
import MobileTopBar from "./MobileTopBar";

describe("MobileActionCluster", () => {
  it("presses and releases the held action", () => {
    const onPress = vi.fn();
    const onRelease = vi.fn();
    render(<MobileActionCluster onActionPress={onPress} onActionRelease={onRelease} />);
    const jump = screen.getByRole("button", { name: "Jump" });
    fireEvent.pointerDown(jump, { pointerId: 1 });
    expect(onPress).toHaveBeenCalledWith("jump");
    fireEvent.pointerUp(jump, { pointerId: 1 });
    expect(onRelease).toHaveBeenCalledWith("jump");
  });

  it("ignores a second pointer while held", () => {
    const onPress = vi.fn();
    render(<MobileActionCluster onActionPress={onPress} />);
    const jump = screen.getByRole("button", { name: "Jump" });
    fireEvent.pointerDown(jump, { pointerId: 1 });
    fireEvent.pointerDown(jump, { pointerId: 2 });
    expect(onPress).toHaveBeenCalledTimes(1);
  });

  it("releases on pointercancel and on unmount", () => {
    const onRelease = vi.fn();
    const cancel = render(<MobileActionCluster onActionRelease={onRelease} />);
    const jump = screen.getByRole("button", { name: "Jump" });
    fireEvent.pointerDown(jump, { pointerId: 1 });
    fireEvent.pointerCancel(jump, { pointerId: 1 });
    expect(onRelease).toHaveBeenCalledWith("jump");

    onRelease.mockClear();
    fireEvent.pointerDown(screen.getByRole("button", { name: "Jump" }), { pointerId: 2 });
    cancel.unmount();
    expect(onRelease).toHaveBeenCalledWith("jump");
  });

  it("renders nothing when every action is hidden", () => {
    const { container } = render(
      <MobileActionCluster actions={[{ id: "jump", label: "Jump", glyph: "jump", hidden: true }]} />,
    );
    expect(container.firstChild).toBeNull();
  });
});

describe("MobileTabBar", () => {
  it("unmounts in landscape by default", () => {
    const { container } = render(<MobileTabBar orientation="landscape" />);
    expect(container.firstChild).toBeNull();
  });

  it("keeps the pill variant in landscape when asked", () => {
    render(<MobileTabBar orientation="landscape" landscape="pill" active="map" />);
    expect(screen.getByRole("button", { name: "Map" })).toHaveAttribute("aria-current", "page");
  });
});

describe("MobileSheet", () => {
  it("renders nothing while closed", () => {
    const { container } = render(<MobileSheet title="Place details" />);
    expect(container.firstChild).toBeNull();
  });

  it("snaps up past the drag threshold and reports the change", () => {
    const onSnapChange = vi.fn();
    render(
      <MobileSheet open title="Place details" defaultSnap="half" onSnapChange={onSnapChange} />,
    );
    const grab = document.querySelector(".msh__grab");
    expect(grab).not.toBeNull();
    fireEvent.pointerDown(grab as Element, { pointerId: 1, clientY: 400 });
    fireEvent.pointerMove(grab as Element, { pointerId: 1, clientY: 300 });
    fireEvent.pointerUp(grab as Element, { pointerId: 1, clientY: 300 });
    expect(onSnapChange).toHaveBeenCalledWith("full");
  });

  it("closes from peek when dragged down", () => {
    const onClose = vi.fn();
    render(<MobileSheet open title="Place details" defaultSnap="peek" onClose={onClose} />);
    const grab = document.querySelector(".msh__grab") as Element;
    fireEvent.pointerDown(grab, { pointerId: 1, clientY: 200 });
    fireEvent.pointerUp(grab, { pointerId: 1, clientY: 300 });
    expect(onClose).toHaveBeenCalled();
  });

  it("uses the drawer arrangement in landscape without a drag handle", () => {
    render(<MobileSheet open orientation="landscape" title="Place details" />);
    expect(document.querySelector(".msh__grab")).toBeNull();
    expect(screen.getByRole("dialog", { name: "Place details" })).toBeInTheDocument();
  });
});

describe("MobileHudFrame", () => {
  it("keeps the controls layer out of the HUD stacking context", () => {
    const { container } = render(
      <MobileHudFrame controlsSlot={<div data-testid="catcher" />} />,
    );
    const controls = container.querySelector(".mhf__controls");
    expect(controls).not.toBeNull();
    expect(controls?.closest(".mhf")).toBeNull();
  });
});

describe("MobileTopBar", () => {
  it("states the empty location honestly", () => {
    render(<MobileTopBar orientation="portrait" />);
    expect(screen.getByText("Unknown parcel")).toBeInTheDocument();
    expect(screen.getByText("no coordinates")).toBeInTheDocument();
  });
});
