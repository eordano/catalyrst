import type { Material, Object3D } from "three";
import type { GLTF } from "three/examples/jsm/loaders/GLTFLoader.js";

export type ItemMetrics = Record<string, number>;
type Geometry = { index?: {count:number}|null; attributes?: {position?: {count:number}}; dispose?:()=>void };
type Animation = {duration:number; tracks:{times:ArrayLike<number>}[]};

export function measureScene(gltf: GLTF, type: "wearable" | "emote"): ItemMetrics {
  const scene = gltf.scene;
  if (type === "emote") {
    const animations = gltf.animations as unknown as Animation[];
    const first = animations[0];
    if (!first || !Number.isFinite(first.duration) || first.duration <= 0 || !first.tracks.length) throw new Error("An emote needs a non-empty animation with a positive duration.");
    const armatures = scene.children.filter(node => node.name.startsWith("Armature"));
    const additionalArmatures = Number(armatures.some(node => node.name === "Armature_Other"));
    if (!additionalArmatures && animations[1] && Math.abs(animations[1].duration-first.duration) > 0.0001) throw new Error("The avatar and prop animations must have the same duration.");
    let avatarMesh = false;
    scene.traverse(node => { if (node.isMesh && /basemesh|avatar_mesh/i.test(node.name)) avatarMesh = true; });
    if (avatarMesh) throw new Error("Remove the avatar mesh from the emote before publishing.");
    const frames = Math.max(...first.tracks.map(track => track.times.length));
    return {sequences:animations.length, duration:first.duration, frames, fps:frames/first.duration,
      props:Number(armatures.some(node => node.name === "Armature_Prop")), additionalArmatures};
  }
  const geometries = new Set<Geometry>();
  const materials = new Set<Material>();
  const textures = new Set<object>();
  let bodies = 0, triangles = 0;
  scene.traverse(node => {
    if (!node.isMesh || !node.geometry) return;
    const geometry = node.geometry as Geometry;
    bodies++; geometries.add(geometry);
    const vertices = geometry.index?.count ?? geometry.attributes?.position?.count;
    if (vertices === undefined || vertices % 3 !== 0) throw new Error("The model contains invalid triangle geometry.");
    triangles += vertices / 3;
    for (const material of node.material ? Array.isArray(node.material) ? node.material : [node.material] : []) {
      materials.add(material);
      for (const value of Object.values(material)) {
        if (value && typeof value === "object" && "isTexture" in value && value.isTexture === true) textures.add(value);
      }
    }
  });
  if (!bodies || !triangles) throw new Error("A wearable model needs visible triangle geometry before it can be published.");
  return {meshes:geometries.size, bodies, materials:materials.size, textures:textures.size, triangles, entities:1};
}

function disposeScene(scene: Object3D) {
  const disposed = new Set<object>();
  scene.traverse(node => {
    if (node.geometry && !disposed.has(node.geometry)) { disposed.add(node.geometry); node.geometry.dispose?.(); }
    for (const material of node.material ? Array.isArray(node.material) ? node.material : [node.material] : []) {
      if (disposed.has(material)) continue;
      disposed.add(material);
      for (const value of Object.values(material)) {
        if (value && typeof value === "object" && "isTexture" in value && value.isTexture === true && !disposed.has(value)) {
          disposed.add(value);
          (value as {dispose?:()=>void}).dispose?.();
          const image = (value as {image?:{close?:()=>void}}).image;
          image?.close?.();
        }
      }
      material.dispose?.();
    }
  });
}

export async function measureItemModels(type: "wearable" | "emote", mainFiles: string[], files: Map<string, Blob>, signal?: AbortSignal, thumbnail?: (blob: Blob) => void): Promise<ItemMetrics> {
  const results: ItemMetrics[] = [];
  for (const main of new Set(mainFiles)) {
    signal?.throwIfAborted();
    const model = files.get(main);
    if (!model) throw new Error(`Missing model: ${main}`);
    if (/\.png$/i.test(main) && type === "wearable") {
      const bitmap = await createImageBitmap(model);
      bitmap.close();
      if (!results.length) thumbnail?.(model);
      results.push({triangles:100, materials:1, textures:1, meshes:1, bodies:1, entities:1});
      continue;
    }
    const {LoadingManager} = await import("three");
    const {GLTFLoader} = await import("three/examples/jsm/loaders/GLTFLoader.js");
    const manager = new LoadingManager();
    const urls = new Map<string,string>();
    const root = new URL(main, "https://item.invalid/");
    manager.setURLModifier(url => {
      if (url.startsWith("data:") || url.startsWith("blob:")) return url;
      const resolved = new URL(url, root);
      const path = decodeURIComponent(resolved.pathname.slice(1));
      if (resolved.origin !== root.origin || !files.has(path)) throw new Error(`Missing or external model resource: ${url}`);
      let objectURL = urls.get(path);
      if (!objectURL) { objectURL=URL.createObjectURL(files.get(path)!); urls.set(path,objectURL); }
      return objectURL;
    });
    let gltf: GLTF | undefined;
    try {
      gltf = await new GLTFLoader(manager).parseAsync(await model.arrayBuffer(), new URL(".", root).href);
      signal?.throwIfAborted();
      const metrics = measureScene(gltf, type);
      if (thumbnail && !results.length && type === "wearable") thumbnail(await renderThumbnail(gltf.scene));
      results.push(metrics);
    } finally {
      if (gltf) disposeScene(gltf.scene);
      for (const url of urls.values()) URL.revokeObjectURL(url);
    }
  }
  if (!results.length) throw new Error("Add an avatar representation before publishing.");
  return Object.fromEntries(Object.keys(results[0]).map(key => [key, Math.max(...results.map(result => result[key]))]));
}


async function renderThumbnail(model: Object3D): Promise<Blob> {
  const Three = await import("three");
  const renderer = new Three.WebGLRenderer({alpha:true, antialias:true, preserveDrawingBuffer:true});
  try {
    renderer.setSize(256, 256, false);
    renderer.setPixelRatio(1);
    const bounds = new Three.Box3().setFromObject(model);
    const center = bounds.getCenter(new Three.Vector3());
    const size = bounds.getSize(new Three.Vector3());
    const radius = Math.max(size.x, size.y, size.z, 0.01);
    const scene = new Three.Scene();
    scene.add(model);
    scene.add(new Three.AmbientLight(0xffffff, 2));
    const light = new Three.DirectionalLight(0xffffff, 3);
    light.position.set(center.x+radius, center.y+radius*2, center.z+radius*2);
    scene.add(light);
    const camera = new Three.PerspectiveCamera(40,1,radius/100,radius*100);
    camera.position.set(center.x+radius*0.8, center.y+radius*0.5, center.z+radius*2.2);
    camera.lookAt(center);
    renderer.render(scene,camera);
    return await new Promise<Blob>((resolve,reject)=>renderer.domElement.toBlob(blob=>blob?resolve(blob):reject(new Error("Could not render a wearable thumbnail.")),"image/png"));
  } finally { renderer.dispose(); renderer.forceContextLoss(); }
}
