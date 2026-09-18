import { cleanup, fireEvent, render, screen, within } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ComponentProps } from "react";

import { DeInspectorPanel } from "./DeInspectorPanel";

afterEach(cleanup);

describe("DeInspectorPanel inline fields", () => {
  it("shows the model path as the scene names it, and explains a model that never loaded", () => {
    const loaded = render(
      <DeInspectorPanel
        name="Wooden Door"
        id="512"
        live
        components={["Transform", "GltfContainer", "GltfContainerLoadingState"]}
        componentValues={{
          GltfContainer: { src: "assets/imported/door/door.glb" },
          GltfContainerLoadingState: { currentState: 4 },
        }}
        onAuthorComponent={() => {}}
      />,
    );
    const body = within(screen.getByRole("region", { name: "Entity components" }));
    expect((body.getByLabelText("src") as HTMLInputElement).value).toBe(
      "assets/imported/door/door.glb",
    );
    expect(body.getByText("Loaded")).toBeTruthy();
    expect(body.queryByText(/No inline fields/)).toBeNull();
    loaded.unmount();

    render(
      <DeInspectorPanel
        name="Door"
        id="513"
        live
        components={["GltfContainerLoadingState"]}
        componentValues={{ GltfContainerLoadingState: { currentState: 2 } }}
      />,
    );
    expect(screen.getByText(/model file is missing/)).toBeTruthy();
  });
});


describe("DeInspectorPanel component picker", () => {
  it("closes after a ribbon reveal and reopens only for a new reveal", () => {
    const authored: string[] = [];
    const props: ComponentProps<typeof DeInspectorPanel> = {
      name: "Model", id: "512", addOpen: true,
      onAuthorComponent: (_id, name) => { authored.push(name); },
    };
    const view = render(<DeInspectorPanel {...props} revealNonce={1} />);
    fireEvent.click(screen.getByTitle("Billboard"));
    expect(authored).toEqual(["Billboard"]);
    expect(screen.queryByTitle("Billboard")).toBeNull();
    expect(screen.getByRole("button", { name: "Add component" }).getAttribute("aria-expanded")).toBe("false");
    view.rerender(<DeInspectorPanel {...props} revealNonce={1} />);
    expect(screen.queryByTitle("Billboard")).toBeNull();
    view.rerender(<DeInspectorPanel {...props} revealNonce={2} />);
    expect(screen.getByTitle("Billboard")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Add component" }));
    expect(screen.queryByTitle("Billboard")).toBeNull();
  });
});

describe("inspector authoring permissions", () => {
  it.each(["GltfContainerLoadingState", "core::GltfContainerLoadingState"])("keeps %s inspectable but prevents edits and deletion", name => {
    const write = vi.fn(), remove = vi.fn(), copy = vi.fn(), paste = vi.fn();
    render(<DeInspectorPanel id="512" live components={[name, "AudioSource"]}
      writableComponents={new Set(["AudioSource"])}
      componentValues={{ [name]: { currentState: 3 }, AudioSource: { audioClipUrl: "tone.wav" } }}
      onAuthorComponent={write} onDeleteComponent={remove} clipboard={{ copy, paste }} />);
    const loading = screen.getByText("Gltf Container Loading State").closest(".eui-comp")! as HTMLElement;
    expect(within(loading).getByRole("button", { name: "Remove component" })).toBeDisabled();
    expect(within(loading).queryByRole("button", { name: "Edit as JSON" })).toBeNull();
    expect(within(loading).queryByRole("button", { name: /paste/i })).toBeNull();
    expect(loading.textContent).toMatch(/Model/);
    const audio = screen.getByText("Audio Source").closest(".eui-comp")! as HTMLElement;
    fireEvent.click(within(audio).getByRole("button", { name: "Remove component" }));
    expect(remove).toHaveBeenCalledExactlyOnceWith("512", "AudioSource");
    expect(write).not.toHaveBeenCalled();
  });
});
