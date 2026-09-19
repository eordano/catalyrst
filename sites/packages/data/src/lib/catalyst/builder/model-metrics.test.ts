import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { measureItemModels, measureScene } from "./model-metrics";
import type { GLTF } from "three/examples/jsm/loaders/GLTFLoader.js";

function model(external=false) {
  const positions=new Float32Array([0,0,0,1,0,0,0,1,0]);
  return new Blob([JSON.stringify({asset:{version:"2.0"},scene:0,scenes:[{nodes:[0]}],nodes:[{mesh:0}],meshes:[{primitives:[{attributes:{POSITION:0}}]}],
    buffers:[{byteLength:36,uri:external?"https://external.invalid/model.bin":`data:application/octet-stream;base64,${Buffer.from(positions.buffer).toString("base64")}`}],
    bufferViews:[{buffer:0,byteOffset:0,byteLength:36}],accessors:[{bufferView:0,componentType:5126,count:3,type:"VEC3",min:[0,0,0],max:[1,1,0]}]})]);
}
describe("publication model metrics",()=>{
  beforeAll(()=>vi.stubGlobal("ProgressEvent",class extends Event {constructor(type:string,init:object){super(type);Object.assign(this,init);}}));
  afterAll(()=>vi.unstubAllGlobals());
  it("measures actual glTF triangles and material instances",async()=>{
    await expect(measureItemModels("wearable",["hat.gltf","hat.gltf"],new Map([["hat.gltf",model()]]))).resolves.toEqual({triangles:1,materials:1,textures:0,meshes:1,bodies:1,entities:1});
  });
  it("rejects missing files, empty geometry and unuploaded external resources",async()=>{
    await expect(measureItemModels("wearable",["hat.gltf"],new Map())).rejects.toThrow("Missing model");
    await expect(measureItemModels("wearable",["hat.gltf"],new Map([["hat.gltf",model(true)]]))).rejects.toThrow("external model resource");
    const empty=new Blob([JSON.stringify({asset:{version:"2.0"},scene:0,scenes:[{}]})]);
    await expect(measureItemModels("wearable",["hat.gltf"],new Map([["hat.gltf",empty]]))).rejects.toThrow("visible triangle geometry");
    await expect(measureItemModels("wearable",[],new Map())).rejects.toThrow("representation");
    await expect(measureItemModels("wearable",["hat.gltf"],new Map([["hat.gltf",model()]]),AbortSignal.abort())).rejects.toThrow();
  });
  it("measures animation duration/tracks and rejects missing or mismatched animations",()=>{
    const scene={children:[{name:"Armature_Prop"}],traverse:()=>{}};
    const gltf={scene,animations:[{duration:2,tracks:[{times:[0,1,2]}]}]} as unknown as GLTF;
    expect(measureScene(gltf,"emote")).toEqual({duration:2,frames:3,fps:1.5,sequences:1,props:1,additionalArmatures:0});
    expect(()=>measureScene({...gltf,animations:[]} as GLTF,"emote")).toThrow("non-empty animation");
    expect(()=>measureScene({...gltf,animations:[...gltf.animations,{duration:4,tracks:[]}]} as GLTF,"emote")).toThrow("same duration");
  });
});
