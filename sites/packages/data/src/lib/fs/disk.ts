export type PickedFile = {
  name: string;
  file: File;
  handle: FileSystemFileHandle | null;
};

type FsaWindow = Window & {
  showOpenFilePicker?: (opts?: {
    multiple?: boolean;
    excludeAcceptAllOption?: boolean;
    types?: { description?: string; accept: Record<string, string[]> }[];
  }) => Promise<FileSystemFileHandle[]>;
  showSaveFilePicker?: (opts?: {
    suggestedName?: string;
    types?: { description?: string; accept: Record<string, string[]> }[];
  }) => Promise<FileSystemFileHandle>;
  showDirectoryPicker?: (opts?: {
    mode?: "read" | "readwrite";
  }) => Promise<FileSystemDirectoryHandle>;
};

function fsaWindow(): FsaWindow | null {
  return typeof window === "undefined" ? null : (window as FsaWindow);
}

function isAbort(e: unknown): boolean {
  return !!e && typeof e === "object" && (e as DOMException).name === "AbortError";
}

export function supportsFileSystemAccess(): boolean {
  const w = fsaWindow();
  return (
    !!w &&
    typeof w.showOpenFilePicker === "function" &&
    typeof w.showSaveFilePicker === "function"
  );
}

export function supportsDirectoryPicker(): boolean {
  const w = fsaWindow();
  return !!w && typeof w.showDirectoryPicker === "function";
}

export type OpenFilesOptions = {
  multiple?: boolean;
  accept?: Record<string, string[]>;
  description?: string;
};

export async function openFiles(opts: OpenFilesOptions = {}): Promise<PickedFile[]> {
  const w = fsaWindow();
  if (!w) return [];
  if (typeof w.showOpenFilePicker === "function") {
    try {
      const handles = await w.showOpenFilePicker({
        multiple: !!opts.multiple,
        excludeAcceptAllOption: false,
        types: opts.accept
          ? [{ description: opts.description ?? "Files", accept: opts.accept }]
          : undefined,
      });
      return await Promise.all(
        handles.map(async (handle) => ({
          name: handle.name,
          handle,
          file: await handle.getFile(),
        })),
      );
    } catch (e) {
      if (isAbort(e)) return [];
    }
  }
  return openFilesViaInput(w, opts);
}

function openFilesViaInput(
  w: FsaWindow,
  opts: OpenFilesOptions,
): Promise<PickedFile[]> {
  return new Promise((resolve) => {
    const input = w.document.createElement("input");
    input.type = "file";
    if (opts.multiple) input.multiple = true;
    if (opts.accept) input.accept = Object.values(opts.accept).flat().join(",");
    input.style.position = "fixed";
    input.style.left = "-9999px";
    let settled = false;
    const done = (out: PickedFile[]) => {
      if (settled) return;
      settled = true;
      resolve(out);
      input.remove();
    };
    input.addEventListener("change", () => {
      const files = Array.from(input.files ?? []);
      done(files.map((file) => ({ name: file.name, handle: null, file })));
    });
    w.addEventListener(
      "focus",
      () => window.setTimeout(() => done([]), 600),
      { once: true },
    );
    w.document.body.appendChild(input);
    input.click();
  });
}

export type OpenDirectoryOutcome =
  | { status: "opened"; handle: FileSystemDirectoryHandle }
  | { status: "cancelled" }
  | { status: "unsupported" };

export async function openDirectoryOutcome(
  mode: "read" | "readwrite" = "read",
): Promise<OpenDirectoryOutcome> {
  const w = fsaWindow();
  if (!w || typeof w.showDirectoryPicker !== "function") {
    return { status: "unsupported" };
  }
  try {
    return { status: "opened", handle: await w.showDirectoryPicker({ mode }) };
  } catch (e) {
    return { status: isAbort(e) ? "cancelled" : "unsupported" };
  }
}

export async function openDirectory(
  mode: "read" | "readwrite" = "read",
): Promise<FileSystemDirectoryHandle | null> {
  const out = await openDirectoryOutcome(mode);
  return out.status === "opened" ? out.handle : null;
}

export async function readDirectoryFiles(
  dir: FileSystemDirectoryHandle,
  prefix = "",
): Promise<Record<string, File>> {
  const out: Record<string, File> = {};
  const entries = (dir as unknown as {
    entries: () => AsyncIterableIterator<[string, FileSystemHandle]>;
  }).entries();
  for await (const [name, handle] of entries) {
    const path = prefix ? `${prefix}/${name}` : name;
    if (handle.kind === "file") {
      out[path] = await (handle as FileSystemFileHandle).getFile();
    } else {
      Object.assign(out, await readDirectoryFiles(handle as FileSystemDirectoryHandle, path));
    }
  }
  return out;
}

export async function readText(f: File): Promise<string> {
  return f.text();
}

export type SaveResult = "written" | "downloaded" | "canceled";

export type SaveTextOptions = {
  onAbort?: "download" | "cancel";
};

export async function saveTextFile(
  suggestedName: string,
  text: string,
  handle?: FileSystemFileHandle | null,
  opts: SaveTextOptions = {},
): Promise<SaveResult> {
  const w = fsaWindow();
  if (!w) return "downloaded";

  const writeTo = async (h: FileSystemFileHandle): Promise<boolean> => {
    const create = (h as unknown as {
      createWritable?: () => Promise<{
        write: (d: string) => Promise<void>;
        close: () => Promise<void>;
      }>;
    }).createWritable;
    if (typeof create !== "function") return false;
    try {
      const writable = await create.call(h);
      await writable.write(text);
      await writable.close();
      return true;
    } catch {
      return false;
    }
  };

  if (handle && (await writeTo(handle))) return "written";

  if (typeof w.showSaveFilePicker === "function") {
    try {
      const h = await w.showSaveFilePicker({ suggestedName });
      if (await writeTo(h)) return "written";
    } catch (e) {
      if (isAbort(e) && opts.onAbort === "cancel") return "canceled";
    }
  }

  const blob = new Blob([text], { type: "application/json" });
  const url = URL.createObjectURL(blob);
  const a = w.document.createElement("a");
  a.href = url;
  a.download = suggestedName;
  w.document.body.appendChild(a);
  a.click();
  a.remove();
  window.setTimeout(() => URL.revokeObjectURL(url), 1000);
  return "downloaded";
}
