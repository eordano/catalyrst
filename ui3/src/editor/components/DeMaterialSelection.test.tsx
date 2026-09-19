import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DeInspectorPanel } from "./DeInspectorPanel";
import { DeMaterialSelection } from "./DeMaterialSelection";
import { changeSelection, selectionChanges, selectionDraft, type MaterialSelection } from "../material-selection";
import { materialComponentError } from "../gltf-materials";

const selection = (): MaterialSelection[] => [
  { entity: "512", name: "Material", value: { tag: "first", material: { pbr: { roughness: 0.2, metallic: 0, albedoColor: { r: 1, g: 0, b: 0, a: 0.3 }, texture: { tex: { texture: { src: "first.png", offset: { x: 0, y: 3 }, tiling: { x: 1, y: 2 } } } } } } } },
  { entity: "513", name: "core::Material", value: { tag: "second", material: { pbr: { roughness: 0.8, metallic: 1, albedoColor: { r: 0, g: 1, b: 0, a: 0.7 }, texture: { tex: { texture: { src: "second.png", offset: { x: 2, y: 4 }, tiling: { x: 3, y: 4 } } } } } } } },
];
const change = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });

describe("selected material authoring", () => {
  it("applies only touched fields to all selected materials through the inspector", async () => {
    const targets = selection();
    const apply = vi.fn().mockResolvedValue(undefined);
    render(<DeInspectorPanel id="512" components={["Material"]} componentValues={{ Material: targets[0]!.value }} materialSelection={targets} onAuthorMaterialSelection={apply} />);
    expect(screen.getByText(/Values differ/)).toBeInTheDocument();
    change("Roughness", "0.4"); change("Albedo color alpha", "0.5"); change("Base texture offset X", "8");
    fireEvent.click(screen.getByRole("button", { name: "Apply to 2 materials" }));
    await waitFor(() => expect(apply).toHaveBeenCalledOnce());
    const changes = apply.mock.calls[0]![0];
    expect(changes.map((item: { entity: string }) => item.entity)).toEqual(["512", "513"]);
    for (const [index, item] of changes.entries()) {
      expect(item.before).toEqual(targets[index]!.value);
      expect(item.value.tag).toBe(index ? "second" : "first");
      expect(item.value.material.pbr).toMatchObject({ roughness: 0.4, metallic: index, albedoColor: { r: index ? 0 : 1, g: index ? 1 : 0, b: 0, a: 0.5 }, texture: { tex: { texture: { src: index ? "second.png" : "first.png", offset: { x: 8, y: index ? 4 : 3 }, tiling: { x: index ? 3 : 1, y: index ? 4 : 2 } } } } });
      expect(materialComponentError(item.value)).toBeNull();
    }
    expect(targets[1]!.value!.material).toEqual(selection()[1]!.value!.material);
  });

  it("sets all RGB channels while preserving each target's alpha", async () => {
    const apply = vi.fn().mockResolvedValue(undefined);
    render(<DeMaterialSelection selection={selection()} onApply={apply} />);
    change("Albedo color", "#0000ff");
    fireEvent.click(screen.getByRole("button", { name: "Apply to 2 materials" }));
    await waitFor(() => expect(apply).toHaveBeenCalledOnce());
    expect(apply.mock.calls[0]![0].map((item: any) => item.value.material.pbr.albedoColor)).toEqual([{ r: 0, g: 0, b: 1, a: 0.3 }, { r: 0, g: 0, b: 1, a: 0.7 }]);
  });

  it("requires an explicit common material type for mixed unions", async () => {
    const targets = selection(); targets[1]!.value = { tag: "second", material: { unlit: { diffuseColor: { r: 1, g: 0, b: 0, a: 1 } } } };
    const apply = vi.fn().mockResolvedValue(undefined);
    render(<DeMaterialSelection selection={targets} onApply={apply} />);
    expect(screen.getByLabelText("Material type")).toHaveValue("mixed");
    expect(screen.getByLabelText("Roughness")).toBeDisabled();
    change("Material type", "unlit"); change("Diffuse color alpha", "0.6");
    fireEvent.click(screen.getByRole("button", { name: "Apply to 2 materials" }));
    await waitFor(() => expect(apply).toHaveBeenCalledOnce());
    for (const item of apply.mock.calls[0]![0]) {
      expect(item.value.material.pbr).toBeUndefined();
      expect(item.value.material.unlit.diffuseColor.a).toBe(0.6);
    }
    expect(apply.mock.calls[0]![0][1].value.tag).toBe("second");
  });

  it("retains a rejected batch draft and blocks external selection changes until discard", async () => {
    const targets = selection();
    let reject!: (reason: Error) => void;
    const apply = vi.fn(() => new Promise<void>((_, no) => { reject = no; }));
    const { rerender } = render(<DeMaterialSelection selection={targets} onApply={apply} />);
    change("Roughness", "0.4");
    fireEvent.click(screen.getByRole("button", { name: "Apply to 2 materials" }));
    expect(screen.getByLabelText("Roughness")).toBeDisabled();
    await act(async () => reject(new Error("Second material rejected; first restored")));
    expect(screen.getByRole("alert")).toHaveTextContent("first restored");
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.4);
    const replacement = selection(); replacement[1]!.entity = "514";
    rerender(<DeMaterialSelection selection={replacement} onApply={apply} />);
    expect(screen.getByRole("button", { name: "Apply to 2 materials" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.2);
  });

  it("does not create a second texture union when selected source types differ", () => {
    const targets = selection(); targets[1]!.value = { material: { pbr: { texture: { tex: { avatarTexture: { userId: "0x123" } } } } } };
    const draft = selectionDraft(targets);
    const next = structuredClone(draft.value);
    (next.material as any).pbr.texture.tex.texture.src = "changed.png";
    expect(() => selectionChanges(changeSelection(draft, next, { paths: [["material", "pbr", "texture", "tex", "texture", "src"]] }))).toThrow(/different sources/);
    const result = selectionChanges(changeSelection(draft, next, { paths: [["material", "pbr", "texture"]] }));
    expect((result[1]!.value.material as any).pbr.texture.tex.avatarTexture).toBeUndefined();
    expect(materialComponentError(result[1]!.value)).toBeNull();
  });

  it("requires every selected entity to have a material", () => {
    const targets = selection(); delete targets[1]!.value;
    render(<DeMaterialSelection selection={targets} onApply={vi.fn()} />);
    expect(screen.getByRole("alert")).toHaveTextContent("Every selected entity");
    expect(screen.getByLabelText("Roughness")).toBeDisabled();
    expect(screen.getByRole("button", { name: "Apply to 2 materials" })).toBeDisabled();
  });
});
