
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
  project?: {
    id: string;
    list: () => Promise<string[]>;
    read: (path: string) => Promise<string>;
    readOnly?: (path: string) => Promise<string>;
    createFileSession?: () => NonNullable<DeWorkspaceCode["project"]>;
    write: (path: string, content: string) => Promise<void>;
    remove?: (path: string) => Promise<void>;
    uiDesigner?: { url: string; runtimeUrl: string };
    assistant?: SceneAssistant;
    assets?: ProjectAssets;
  };
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

export type SceneAssistantEvent =
  | { type: "started"; turnId: string; provider: string; conversationId?: string }
  | { type: "session"; sessionId: string }
  | { type: "text"; text: string }
  | { type: "tool"; name: string; detail: string }
  | { type: "error"; message: string }
  | { type: "done"; exitCode: number | null; cancelled: boolean };

export interface AssistantConversation {
  id: string; provider: string; title: string; updatedAt: number; truncated: boolean;
}
export interface AssistantSelection { id: string; name: string }
export type AssistantHistoryEvent = SceneAssistantEvent | { type: "user"; text: string; selectedEntities: AssistantSelection[] };
export interface SceneAssistant {
  conversations: () => Promise<{ conversations: AssistantConversation[] }>;
  conversation: (id: string) => Promise<AssistantConversation & { events: AssistantHistoryEvent[]; resumable: boolean }>;
  deleteConversation: (id: string) => Promise<void>;
  providers: () => Promise<{ providers: { id: string; label: string; available: boolean }[]; sceneTools: { available: boolean; url: string | null; paired?: boolean; bridge?: string | null }; busy: boolean }>;
  turn: (input: { provider: string; prompt: string; conversationId?: string; selectedEntities?: AssistantSelection[] }, onEvent: (event: SceneAssistantEvent) => void, signal: AbortSignal) => Promise<void>;
  cancel: (turnId: string) => Promise<void>;
}

export type AuthorComponentFn = (
  entity: string | number | null | undefined,
  name: string,
  json: string,
) => void;

export type DeleteComponentFn = (entity: string | number, name: string) => void;

export type AuthorComponentsFn = (entity: string | number | null | undefined, changes: { name: string; json: string }[]) => Promise<void>;

export interface ProjectAssets {
  preparePreview?(): Promise<Record<string, string>>;
  revision?(path: string): Promise<string>;
  list(): Promise<{ path: string; size: number }[]>;
  read(path: string): Promise<{ content: ArrayBuffer; revision: string }>;
  write(path: string, content: ArrayBuffer, revision?: string): Promise<void>;
  remove(path: string, revision: string): Promise<void>;
}
