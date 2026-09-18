import { z } from "zod";

export async function validateModelFile(model: File, resources?: Set<string>): Promise<void> {
    if (model.size > 20 * 1024 * 1024 || model.size === 0) throw new Error("Choose a model smaller than 20 MB.");
    if (/\.glb$/i.test(model.name)) {
      if (model.size < 12) throw new Error("The selected file is not a GLB 2 model.");
      const header = new DataView(await model.slice(0, 12).arrayBuffer());
      if (header.getUint32(0, true) !== 0x46546c67 || header.getUint32(4, true) !== 2 || header.getUint32(8, true) !== model.size) {
        throw new Error("The selected file is not a GLB 2 model.");
      }
    } else if (/\.gltf$/i.test(model.name)) {
      const document = z.object({
        asset: z.object({ version: z.literal("2.0") }),
        buffers: z.array(z.object({ uri: z.string().optional() })).optional(),
        images: z.array(z.object({ uri: z.string().optional() })).optional(),
      }).safeParse(JSON.parse(await model.text()));
      if (!document.success) throw new Error("The selected file is not a glTF 2 model.");
      for (const part of [...(document.data.buffers ?? []), ...(document.data.images ?? [])]) {
        if (!part.uri || part.uri.startsWith("data:")) continue;
        const url = new URL(part.uri, `https://item.invalid/${model.name}`);
        if (url.origin !== "https://item.invalid" || !resources?.has(decodeURIComponent(url.pathname.slice(1)))) {
          throw new Error("This glTF references separate files. Include its missing resources in a ZIP or export a GLB with embedded textures.");
        }
      }
    } else throw new Error("Choose a .glb or embedded .gltf model.");
}
