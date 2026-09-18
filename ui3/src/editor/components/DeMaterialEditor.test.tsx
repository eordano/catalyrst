import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DeInspectorPanel } from "./DeInspectorPanel";
import { DeMaterialEditor } from "./DeMaterialEditor";
import { materialComponentError } from "../gltf-materials";
import type { ProjectAssets } from "../types";

const pbr = { authoredMetadata: "keep", material: { pbr: { metallic: 0, roughness: 0.5, castShadows: false, albedoColor: { r: 0.2, g: 0.4, b: 0.6, a: 0.8 }, customMetadata: "keep" } } };
const assets: ProjectAssets = { list: vi.fn(async () => [{ path: "assets/stone.png", size: 4 }, { path: "assets/scene.glb", size: 8 }]), read: vi.fn(), write: vi.fn(), remove: vi.fn() };
const change = (label: string, value: string) => fireEvent.change(screen.getByLabelText(label), { target: { value } });

describe("primitive material inspector", () => {
  it.each(["Material", "core::Material"])("applies complete PBR values and project textures through ACK for %s", async name => {
    const author = vi.fn().mockResolvedValue(undefined);
    render(<DeInspectorPanel id="512" components={[name]} componentValues={{ [name]: pbr }} assets={assets} onAuthorComponents={author} />);
    expect(screen.getByLabelText("Metallic")).toHaveValue(0);
    expect(screen.getByLabelText("Cast material shadows")).not.toBeChecked();
    change("Specular intensity", "0"); change("Direct intensity", "0.2"); change("Emissive intensity", "3");
    change("Emissive color", "#00ff00"); change("Reflectivity color", "#ff0000");
    change("Albedo color alpha", "0.4"); change("Alpha cutoff", "0.3"); change("Transparency", "0");
    change("Base texture source", "texture");
    await screen.findByRole("option", { name: "assets/stone.png" });
    change("Base texture project asset", "assets/stone.png");
    change("Base texture tiling Y", "2"); change("Base texture filter", "2");
    change("Normal texture source", "texture"); change("Normal texture path or URL", "https://example.com/normal.png");
    expect(author).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Apply material" }));
    await waitFor(() => expect(author).toHaveBeenCalledOnce());
    const [entity, changes] = author.mock.calls[0]!;
    expect(entity).toBe("512"); expect(changes[0].name).toBe(name);
    const result = JSON.parse(changes[0].json);
    expect(materialComponentError(result)).toBeNull();
    expect(result).toMatchObject({ authoredMetadata: "keep", material: { $case: "pbr", pbr: { metallic: 0, roughness: 0.5, castShadows: false, customMetadata: "keep", specularIntensity: 0, directIntensity: 0.2, emissiveIntensity: 3, emissiveColor: { r: 0, g: 1, b: 0 }, reflectivityColor: { r: 1, g: 0, b: 0 }, albedoColor: { r: 0.2, g: 0.4, b: 0.6, a: 0.4 }, alphaTest: 0.3, transparencyMode: 0, texture: { tex: { $case: "texture", texture: { src: "assets/stone.png", filterMode: 2, tiling: { x: 1, y: 2 } } } }, bumpTexture: { tex: { texture: { src: "https://example.com/normal.png" } } } } } });
  });

  it("switches to unlit with avatar and video texture unions", async () => {
    const apply = vi.fn().mockResolvedValue(undefined);
    render(<DeMaterialEditor value={pbr} onApply={apply} />);
    change("Material type", "unlit"); change("Diffuse color", "#ffffff"); change("Diffuse color alpha", "0.5"); change("Alpha cutoff", "0");
    change("Base texture source", "videoTexture"); change("Base texture video entity", "513");
    change("Alpha texture source", "avatarTexture"); change("Alpha texture avatar user ID", "0x123");
    fireEvent.click(screen.getByRole("button", { name: "Apply material" }));
    await waitFor(() => expect(apply).toHaveBeenCalledOnce());
    const result = apply.mock.calls[0]![0];
    expect(materialComponentError(result)).toBeNull();
    expect(result.material.pbr).toBeUndefined();
    expect(result).toMatchObject({ authoredMetadata: "keep", material: { $case: "unlit", unlit: { castShadows: false, alphaTest: 0, diffuseColor: { r: 1, g: 1, b: 1, a: 0.5 }, texture: { tex: { $case: "videoTexture", videoTexture: { videoPlayerEntity: 513 } } }, alphaTexture: { tex: { $case: "avatarTexture", avatarTexture: { userId: "0x123" } } } } } });
  });

  it("retains rejected drafts and prevents concurrent writes until acknowledgment", async () => {
    let reject!: (reason: Error) => void;
    const apply = vi.fn(() => new Promise<void>((_, no) => { reject = no; }));
    render(<DeMaterialEditor value={pbr} onApply={apply} />);
    change("Roughness", "0.1");
    fireEvent.click(screen.getByRole("button", { name: "Apply material" }));
    expect(screen.getByRole("button", { name: "Applying\u2026" })).toBeDisabled();
    expect(screen.getByLabelText("Roughness")).toBeDisabled();
    await act(async () => reject(new Error("Native material write rejected")));
    expect(screen.getByRole("alert")).toHaveTextContent("Native material write rejected");
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.1);
    expect(pbr.material.pbr.roughness).toBe(0.5);
    fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.5);
    expect(apply).toHaveBeenCalledOnce();
  });

  it("blocks malformed textures and external conflicts, and remains read-only without ACK capability", () => {
    const apply = vi.fn();
    const { rerender } = render(<DeMaterialEditor value={pbr} onApply={apply} />);
    change("Base texture source", "texture"); change("Base texture path or URL", "../secret.png");
    expect(screen.getByRole("alert")).toHaveTextContent("project texture path");
    expect(screen.getByRole("button", { name: "Apply material" })).toBeDisabled();
    change("Base texture source", "none");
    const current = { material: { pbr: { roughness: 0.9 } } };
    rerender(<DeMaterialEditor value={current} onApply={apply} />);
    expect(screen.getByRole("alert")).toHaveTextContent("component changed in the scene");
    expect(screen.getByRole("button", { name: "Apply material" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Discard changes" }));
    expect(screen.getByLabelText("Roughness")).toHaveValue(0.9);
    rerender(<DeMaterialEditor value={current} />);
    expect(screen.getByLabelText("Material type")).toBeDisabled();
    expect(apply).not.toHaveBeenCalled();
  });
});
