import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DeGltfNodeModifiers } from "./DeGltfNodeModifiers";
import { DeInspectorPanel } from "./DeInspectorPanel";
import { materialSwapError, materialValue, modifiersValue, newMaterialSwap, textureAssetPaths, withMaterial } from "../gltf-materials";
import type { ProjectAssets } from "../types";

const assets = (list = vi.fn(async () => [{ path: "assets/wood.png", size: 8 }, { path: "assets/model.glb", size: 12 }])): ProjectAssets => ({ list, read: vi.fn(), write: vi.fn(), remove: vi.fn() });
const empty = { modifiers: [] };

describe("GLTF material swaps", () => {
  it("submits upstream material and texture unions through the acknowledged component path", async () => {
    const author = vi.fn().mockResolvedValue(undefined);
    render(<DeInspectorPanel id="512" components={["GltfNodeModifiers"]} componentValues={{ GltfNodeModifiers: empty, GltfContainerLoadingState: { nodePaths: ["Scene/Tree/Leaves"] } }} assets={assets()} onAuthorComponents={author} />);
    fireEvent.click(screen.getByRole("button", { name: "Add material swap" }));
    fireEvent.change(screen.getByLabelText("Swap 1 node path"), { target: { value: "Scene/Tree/Leaves" } });
    fireEvent.change(screen.getByLabelText("Metallic"), { target: { value: "0" } });
    fireEvent.change(screen.getByLabelText("Base texture source"), { target: { value: "texture" } });
    await screen.findByRole("option", { name: "assets/wood.png" });
    fireEvent.change(screen.getByLabelText("Base texture project asset"), { target: { value: "assets/wood.png" } });
    fireEvent.change(screen.getByLabelText("Base texture tiling X"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("Base texture wrap"), { target: { value: "2" } });
    fireEvent.change(screen.getByLabelText("Base texture filter"), { target: { value: "1" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply material swaps" }));
    await waitFor(() => expect(author).toHaveBeenCalledOnce());
    const [entity, changes] = author.mock.calls[0]!;
    expect(entity).toBe("512");
    expect(changes[0].name).toBe("GltfNodeModifiers");
    const value = JSON.parse(changes[0].json);
    expect(materialSwapError(value)).toBeNull();
    expect(value.modifiers[0]).toMatchObject({ path: "Scene/Tree/Leaves", castShadows: true, material: { material: { $case: "pbr", pbr: { metallic: 0, texture: { tex: { $case: "texture", texture: { src: "assets/wood.png", wrapMode: 2, filterMode: 1, offset: { x: 0, y: 0 }, tiling: { x: 2, y: 1 } } } } } } } });
    expect(screen.queryByRole("option", { name: "assets/model.glb" })).not.toBeInTheDocument();
  });

  it("retains rejected changes, prevents concurrent apply, and discards to acknowledged state", async () => {
    let reject!: (error: Error) => void;
    const apply = vi.fn(() => new Promise<void>((_resolve, no) => { reject = no; }));
    const value = { modifiers: [newMaterialSwap()] };
    render(<DeGltfNodeModifiers value={value} onApply={apply} />);
    fireEvent.change(screen.getByLabelText("Roughness"), { target: { value: "0.2" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply material swaps" }));
    expect(screen.getByRole("button", { name: "Applying\u2026" })).toBeDisabled();
    expect(screen.getByLabelText("Roughness")).toBeDisabled();
    reject(new Error("Engine rejected the material override"));
    await screen.findByRole("alert");
    expect(screen.getByRole("alert")).toHaveTextContent("Engine rejected");
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.2);
    expect(materialValue(value.modifiers[0]!).value.roughness).toBe(0.5);
    fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.5);
    expect(screen.queryByRole("alert")).not.toBeInTheDocument();
    expect(apply).toHaveBeenCalledOnce();
  });

  it("edits unlit avatar and video texture references without losing other swaps or unknown fields", async () => {
    const apply = vi.fn().mockResolvedValue(undefined);
    const untouched = { ...newMaterialSwap(), path: "Other/Node", customMetadata: "keep" };
    render(<DeGltfNodeModifiers value={{ authoredMetadata: "keep", modifiers: [newMaterialSwap(), untouched] }} onApply={apply} />);
    fireEvent.change(screen.getAllByLabelText("Material type")[0]!, { target: { value: "unlit" } });
    fireEvent.change(screen.getAllByLabelText("Base texture source")[0]!, { target: { value: "videoTexture" } });
    fireEvent.change(screen.getByLabelText("Base texture video entity"), { target: { value: "513" } });
    fireEvent.change(screen.getAllByLabelText("Alpha texture source")[0]!, { target: { value: "avatarTexture" } });
    fireEvent.change(screen.getByLabelText("Alpha texture avatar user ID"), { target: { value: "0x123" } });
    fireEvent.click(screen.getByRole("button", { name: "Apply material swaps" }));
    await waitFor(() => expect(apply).toHaveBeenCalledOnce());
    const next = apply.mock.calls[0]![0];
    expect(materialSwapError(next)).toBeNull();
    expect(next.authoredMetadata).toBe("keep");
    expect(next.modifiers[1]).toEqual(untouched);
    expect(next.modifiers[0].material.material).toMatchObject({ $case: "unlit", unlit: { texture: { tex: { $case: "videoTexture", videoTexture: { videoPlayerEntity: 513 } } }, alphaTexture: { tex: { $case: "avatarTexture", avatarTexture: { userId: "0x123" } } } } });
  });

  it("blocks stale draft writes and loads current scene values on discard", () => {
    const apply = vi.fn();
    const value = { modifiers: [newMaterialSwap()] };
    const { rerender } = render(<DeGltfNodeModifiers value={value} onApply={apply} />);
    fireEvent.change(screen.getByLabelText("Roughness"), { target: { value: "0.2" } });
    rerender(<DeGltfNodeModifiers value={{ modifiers: [withMaterial(value.modifiers[0]!, "pbr", { roughness: 0.8 })] }} onApply={apply} />);
    expect(screen.getByRole("alert")).toHaveTextContent("component changed in the scene");
    expect(screen.getByRole("button", { name: "Apply material swaps" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.8);
  });

  it("reports texture listing failures and permits retry without dropping a draft", async () => {
    const list = vi.fn().mockRejectedValueOnce(new Error("Folder permission expired")).mockResolvedValueOnce([{ path: "assets/wood.png", size: 8 }]);
    render(<DeGltfNodeModifiers value={{ modifiers: [newMaterialSwap()] }} assets={assets(list)} onApply={vi.fn()} />);
    fireEvent.change(screen.getByLabelText("Base texture source"), { target: { value: "texture" } });
    await screen.findByRole("button", { name: "Retry textures" });
    fireEvent.click(screen.getByRole("button", { name: "Retry textures" }));
    await screen.findByRole("option", { name: "assets/wood.png" });
    expect(screen.getByLabelText("Base texture source")).toHaveValue("texture");
  });

  it("validates malformed textures and preserves numeric zero values", () => {
    expect(textureAssetPaths([{ path: "z/model.GLB" }, { path: "a/image.PNG" }])).toEqual(["a/image.PNG"]);
    const value = modifiersValue({ modifiers: [withMaterial(newMaterialSwap(), "pbr", { metallic: 0, roughness: 0, texture: { tex: { texture: { src: "../secret.png" } } } })] });
    expect(materialSwapError(value)).toMatch(/project texture path/);
    value.modifiers[0] = withMaterial(value.modifiers[0]!, "pbr", { metallic: 0, roughness: 0, texture: { tex: { texture: { src: "https://example.com/texture.png", filterMode: "linear" } } } });
    expect(materialSwapError(value)).toMatch(/supported value/);
  });
});
