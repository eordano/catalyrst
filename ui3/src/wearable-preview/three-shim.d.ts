declare module "three" {
  export class Vector3 {
    x: number;
    y: number;
    z: number;
    set(x: number, y: number, z: number): this;
  }

  export class Color {
    constructor(r: number, g: number, b: number);
    constructor(color?: string | number);
    copy(color: Color): this;
  }

  export interface Material {
    name?: string;
    color?: Color;
    metalness?: number;
    roughness?: number;
    needsUpdate?: boolean;
    dispose?(): void;
    [key: string]: unknown;
  }

  export interface BufferGeometry {
    dispose?(): void;
  }

  export class Euler {
    x: number;
    y: number;
    z: number;
    set(x: number, y: number, z: number): this;
  }

  export class Object3D {
    name: string;
    parent: Object3D | null;
    visible: boolean;
    position: Vector3;
    rotation: Euler;
    isMesh?: boolean;
    material?: Material | Material[];
    geometry?: BufferGeometry;
    add(...objects: Object3D[]): this;
    traverse(callback: (object: Object3D) => void): void;
  }

  export class Group extends Object3D {}
  export class Scene extends Object3D {}
  export class Camera extends Object3D {}

  export class Texture {
    colorSpace: string;
    flipY: boolean;
    needsUpdate: boolean;
    dispose(): void;
  }

  export class CanvasTexture extends Texture {
    constructor(canvas: HTMLCanvasElement);
  }

  export class TextureLoader {
    loadAsync(url: string): Promise<Texture>;
  }

  export interface MeshStandardMaterialParameters {
    name?: string;
    map?: Texture | null;
    transparent?: boolean;
    roughness?: number;
    metalness?: number;
  }

  export class MeshStandardMaterial {
    constructor(parameters?: MeshStandardMaterialParameters);
    [key: string]: unknown;
    name: string;
    color: Color;
    emissive: Color;
    emissiveMap: Texture | null;
    needsUpdate: boolean;
    dispose(): void;
  }

  export class CylinderGeometry {
    constructor(
      radiusTop?: number,
      radiusBottom?: number,
      height?: number,
      radialSegments?: number,
    );
    dispose(): void;
  }

  export interface MeshBasicMaterialParameters {
    color?: number | string;
    map?: Texture;
  }

  export class MeshBasicMaterial {
    constructor(parameters?: MeshBasicMaterialParameters);
    dispose(): void;
  }

  export class Mesh extends Object3D {
    constructor(
      geometry?: BufferGeometry | CylinderGeometry,
      material?: MeshBasicMaterial | MeshBasicMaterial[],
    );
  }

  export class PerspectiveCamera extends Camera {
    constructor(fov?: number, aspect?: number, near?: number, far?: number);
    fov: number;
    aspect: number;
    near: number;
    far: number;
    updateProjectionMatrix(): void;
  }

  export class HemisphereLight extends Object3D {
    constructor(skyColor?: number | string, groundColor?: number | string, intensity?: number);
  }

  export class DirectionalLight extends Object3D {
    constructor(color?: number | string, intensity?: number);
  }

  export interface KeyframeTrack {
    name: string;
  }

  export interface AnimationClip {
    tracks: KeyframeTrack[];
    duration: number;
  }

  export interface AnimationAction {
    play(): AnimationAction;
  }

  export class AnimationMixer {
    constructor(root: Object3D);
    update(deltaSeconds: number): void;
    stopAllAction(): void;
    clipAction(clip: AnimationClip): AnimationAction;
  }

  export class Clock {
    getDelta(): number;
  }

  export class Box3 {
    min: Vector3;
    setFromObject(object: Object3D): this;
    isEmpty(): boolean;
    getSize(target: Vector3): Vector3;
    getCenter(target: Vector3): Vector3;
  }

  export class LoadingManager {
    setURLModifier(callback: (url: string) => string): this;
  }

  export interface WebGLRendererParameters {
    antialias?: boolean;
    alpha?: boolean;
  }

  export class WebGLRenderer {
    constructor(parameters?: WebGLRendererParameters);
    domElement: HTMLCanvasElement;
    outputColorSpace: string;
    setPixelRatio(value: number): void;
    setClearColor(color: Color | string | number, alpha?: number): void;
    setSize(width: number, height: number, updateStyle?: boolean): void;
    setAnimationLoop(callback: (() => void) | null): void;
    render(scene: Object3D, camera: Camera): void;
    dispose(): void;
  }

  export const SRGBColorSpace: string;

  export const MathUtils: {
    degToRad(degrees: number): number;
  };
}

declare module "three/examples/jsm/loaders/GLTFLoader.js" {
  import type { AnimationClip, LoadingManager, Object3D } from "three";

  export interface GLTF {
    scene: Object3D;
    animations: AnimationClip[];
  }

  export class GLTFLoader {
    constructor(manager?: LoadingManager);
    loadAsync(url: string): Promise<GLTF>;
  }
}

declare module "three/examples/jsm/controls/OrbitControls.js" {
  import type { Camera, Vector3 } from "three";

  export class OrbitControls {
    constructor(camera: Camera, domElement?: HTMLElement);
    enablePan: boolean;
    enableDamping: boolean;
    dampingFactor: number;
    enabled: boolean;
    enableZoom: boolean;
    enableRotate: boolean;
    autoRotate: boolean;
    autoRotateSpeed: number;
    minDistance: number;
    maxDistance: number;
    maxPolarAngle: number;
    target: Vector3;
    update(): void;
    dispose(): void;
  }
}
