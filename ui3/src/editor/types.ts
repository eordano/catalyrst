
export interface EditorVec {
  x: number;
  y: number;
  z: number;
  w?: number;
}

export interface EditorTransform {
  position?: EditorVec;
  rotation?: EditorVec;
  scale?: EditorVec;
}

export interface DeTreeNode {
  id: string | number;
  name: string;
  selected?: boolean;
  expanded?: boolean;
  children?: DeTreeNode[];
}

export interface DeInspector {
  name?: string;
  id?: string | number;
  components?: string[] | null;
  transform?: EditorTransform | null;
}

export interface DeCatalogItem {
  id: string | number;
  name: string;
  pack?: string;
  hue?: number;
  category?: string;
  thumbnailUrl?: string;
  glbFile?: string;
  glbUrl?: string;
  src?: string;
  contents?: Record<string, string> | null;
  smart?: boolean;
}

export interface DeLocalItem {
  path: string;
  folder?: string;
}

export interface DeWorkspaceCode {
  typesUrl?: string;
  virtualFiles?: { path: string; text: string }[];
  getDir?: () => Promise<FileSystemDirectoryHandle | null>;
  hydrate?: () => Promise<Record<string, string> | null>;
  persist?: (path: string, text: string) => Promise<void> | void;
}

export interface CameraPrefs {
  preset: string;
  sensitivity: { orbit: number; pan: number; zoom: number };
  invertY: boolean;
}

export type AuthorComponentFn = (
  entity: string | number | null | undefined,
  name: string,
  json: string,
) => void;

export type DeleteComponentFn = (entity: string | number, name: string) => void;
