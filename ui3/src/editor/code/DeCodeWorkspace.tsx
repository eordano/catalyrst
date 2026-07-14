import type { IStandaloneCodeEditor } from "monaco-editor";
import { useEffect, useRef, useState } from "react";
import { loadMonaco, modelFor, languageFor } from "./monaco-host";
import type { DeWorkspaceCode } from "../types";
import "./decode.css";

type Monaco = typeof import("monaco-editor");

interface FileTreeNode {
  path: string;
  name: string;
  kind: "dir" | "file";
  children?: FileTreeNode[];
}

interface DirNode {
  path: string;
  name: string;
  kind: "dir";
  children: FileTreeNode[];
}

const SKIP_DIRS = new Set(["node_modules", ".git", "bin", "dist", ".DS_Store"]);
const MAX_FILES = 400;
const MAX_DEPTH = 7;

function starterFor(path: string): string {
  if (/\.tsx?$/.test(path)) return `// ${path}\n\nexport {}\n`;
  if (path.endsWith(".json")) return "{\n}\n";
  return "";
}

async function readDirTree(
  dir: FileSystemDirectoryHandle,
  prefix = "",
  depth = 0,
  budget: { n: number } = { n: 0 },
): Promise<FileTreeNode[]> {
  if (depth > MAX_DEPTH) return [];
  const out: FileTreeNode[] = [];
  for await (const entry of dir.values()) {
    if (SKIP_DIRS.has(entry.name)) continue;
    if (budget.n >= MAX_FILES) break;
    const path = prefix ? `${prefix}/${entry.name}` : entry.name;
    if (entry.kind === "directory") {
      const children = await readDirTree(entry, path, depth + 1, budget);
      out.push({ path, name: entry.name, kind: "dir", children });
    } else {
      budget.n += 1;
      out.push({ path, name: entry.name, kind: "file" });
    }
  }
  out.sort((a, b) => (a.kind !== b.kind ? (a.kind === "dir" ? -1 : 1) : a.name.localeCompare(b.name)));
  return out;
}

function virtualTree(files: { path: string; text: string }[]): FileTreeNode[] {
  const root: FileTreeNode[] = [];
  const dirs = new Map<string, DirNode>();
  const dirFor = (segs: string[]): FileTreeNode[] => {
    if (segs.length === 0) return root;
    const key = segs.join("/");
    const cached = dirs.get(key);
    if (cached) return cached.children;
    const parent = dirFor(segs.slice(0, -1));
    const node: DirNode = { path: key, name: segs[segs.length - 1]!, kind: "dir", children: [] };
    parent.push(node);
    dirs.set(key, node);
    return node.children;
  };
  for (const f of files) {
    const segs = f.path.split("/");
    dirFor(segs.slice(0, -1)).push({ path: f.path, name: segs[segs.length - 1]!, kind: "file" });
  }
  const sortRec = (nodes: FileTreeNode[]) => {
    nodes.sort((a, b) => (a.kind !== b.kind ? (a.kind === "dir" ? -1 : 1) : a.name.localeCompare(b.name)));
    for (const n of nodes) if (n.children) sortRec(n.children);
  };
  sortRec(root);
  return root;
}

async function fileHandleFor(
  dir: FileSystemDirectoryHandle,
  path: string,
  create = false,
): Promise<FileSystemFileHandle> {
  const segs = path.split("/");
  let cur: FileSystemDirectoryHandle = dir;
  for (let i = 0; i < segs.length - 1; i++) {
    cur = await cur.getDirectoryHandle(segs[i]!, { create });
  }
  return cur.getFileHandle(segs[segs.length - 1]!, { create });
}

interface TreeNodeProps {
  node: FileTreeNode;
  depth: number;
  openPath: string | null;
  dirty: Set<string>;
  onOpen: (path: string) => void;
}

function TreeNode({ node, depth, openPath, dirty, onOpen }: TreeNodeProps) {
  const [collapsed, setCollapsed] = useState(false);
  if (node.kind === "dir") {
    return (
      <>
        <button
          type="button"
          className="decode-row decode-row-dir"
          style={{ paddingLeft: 8 + depth * 14 }}
          onClick={() => setCollapsed((v) => !v)}
        >
          <span className="decode-caret">{collapsed ? "▸" : "▾"}</span> {node.name}
        </button>
        {!collapsed &&
          node.children?.map((c) => (
            <TreeNode key={c.path} node={c} depth={depth + 1} openPath={openPath} dirty={dirty} onOpen={onOpen} />
          ))}
      </>
    );
  }
  return (
    <button
      type="button"
      className={`decode-row decode-row-file${openPath === node.path ? " active" : ""}`}
      style={{ paddingLeft: 8 + depth * 14 }}
      onClick={() => onOpen(node.path)}
      title={node.path}
    >
      {node.name}
      {dirty.has(node.path) ? <span className="decode-dirty">●</span> : null}
    </button>
  );
}

export interface DeCodeWorkspaceProps {
  code?: DeWorkspaceCode;
  store?: Map<string, string>;
  onClose?: () => void;
}

export default function DeCodeWorkspace({ code = {}, store, onClose }: DeCodeWorkspaceProps) {
  const { typesUrl = null, virtualFiles = [], getDir = null, hydrate = null, persist = null } = code;
  const hostRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const dirRef = useRef<FileSystemDirectoryHandle | null>(null);
  const virtualsRef = useRef<Map<string, string>>(store ?? new Map());
  const [status, setStatus] = useState<string | null>("Loading editor…");
  const [tree, setTree] = useState<FileTreeNode[]>([]);
  const [source, setSource] = useState<"disk" | "virtual">("virtual");
  const [openPath, setOpenPath] = useState<string | null>(null);
  const [dirty, setDirty] = useState<Set<string>>(new Set());
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [typesLoaded, setTypesLoaded] = useState(0);
  const openPathRef = useRef<string | null>(null);
  openPathRef.current = openPath;

  useEffect(() => {
    let dead = false;
    (async () => {
      try {
        const { monaco, typesLoaded: n } = await loadMonaco(typesUrl ?? undefined);
        if (dead) return;
        monacoRef.current = monaco;
        setTypesLoaded(n);
        const editor = monaco.editor.create(hostRef.current!, {
          theme: "vs-dark",
          automaticLayout: true,
          minimap: { enabled: false },
          fontSize: 13,
          scrollBeyondLastLine: false,
          fixedOverflowWidgets: true,
        });
        editorRef.current = editor;
        editor.addCommand(monaco.KeyMod.CtrlCmd | monaco.KeyCode.KeyS, () => saveCurrent());
        editor.onDidChangeModelContent(() => {
          const p = openPathRef.current;
          if (p) setDirty((d) => (d.has(p) ? d : new Set(d).add(p)));
        });

        let dir: FileSystemDirectoryHandle | null = null;
        try {
          dir = typeof getDir === "function" ? await getDir() : null;
        } catch {
          dir = null;
        }
        if (dir) {
          dirRef.current = dir;
          setSource("disk");
          setTree(await readDirTree(dir));
        } else {
          for (const f of virtualFiles) {
            if (!virtualsRef.current.has(f.path)) virtualsRef.current.set(f.path, f.text);
          }
          if (typeof hydrate === "function") {
            try {
              const stored = await hydrate();
              for (const [p, text] of Object.entries(stored ?? {})) {
                if (typeof text === "string") virtualsRef.current.set(p, text);
              }
            } catch {
            }
          }
          setSource("virtual");
          setTree(virtualTree([...virtualsRef.current].map(([p, text]) => ({ path: p, text }))));
        }
        setStatus(null);
        const first = virtualsRef.current.has("src/index.ts") || virtualFiles.some((f) => f.path === "src/index.ts") ? "src/index.ts" : null;
        if (dir) {
          openFile("src/index.ts").catch(() => {});
        } else if (first) {
          openFile(first).catch(() => {});
        }
      } catch (e) {
        if (!dead) setStatus(`Editor failed to load: ${e instanceof Error ? e.message : String(e)}`);
      }
    })();
    return () => {
      dead = true;
      try {
        editorRef.current?.dispose();
      } catch {
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function openFile(path: string) {
    const monaco = monacoRef.current;
    const editor = editorRef.current;
    if (!monaco || !editor) return;
    let text: string | null | undefined = null;
    if (dirRef.current) {
      try {
        const fh = await fileHandleFor(dirRef.current, path);
        text = await (await fh.getFile()).text();
      } catch {
        return;
      }
    } else {
      text = virtualsRef.current.get(path);
      if (text === undefined) return;
    }
    const model = modelFor(monaco, path, text);
    editor.setModel(model);
    setOpenPath(path);
  }

  async function saveCurrent(): Promise<void> {
    const p = openPathRef.current;
    const monaco = monacoRef.current;
    if (!p || !monaco) return;
    const uri = monaco.Uri.parse(`file:///${p}`);
    const model = monaco.editor.getModel(uri);
    if (!model) return;
    const text = model.getValue();
    if (dirRef.current) {
      try {
        const fh = await fileHandleFor(dirRef.current, p, true);
        const w = await fh.createWritable();
        await w.write(text);
        await w.close();
      } catch (e) {
        setStatus(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
        return;
      }
    } else {
      virtualsRef.current.set(p, text);
      if (typeof persist === "function") {
        try {
          await persist(p, text);
        } catch (e) {
          setStatus(`Save failed: ${e instanceof Error ? e.message : String(e)}`);
          return;
        }
      }
    }
    setDirty((d) => {
      const nd = new Set(d);
      nd.delete(p);
      return nd;
    });
  }

  async function createFile(rawPath: string): Promise<void> {
    const path = rawPath.trim().replace(/^\/+/, "").replace(/\/+$/, "");
    if (!path) return;
    if (dirRef.current) {
      try {
        await fileHandleFor(dirRef.current, path, true);
        setTree(await readDirTree(dirRef.current));
      } catch (e) {
        setStatus(`Create failed: ${e instanceof Error ? e.message : String(e)}`);
        return;
      }
    } else {
      if (!virtualsRef.current.has(path)) {
        virtualsRef.current.set(path, starterFor(path));
        if (typeof persist === "function") {
          try {
            await persist(path, starterFor(path));
          } catch {
          }
        }
      }
      setTree(virtualTree([...virtualsRef.current].map(([p, text]) => ({ path: p, text }))));
    }
    setCreating(false);
    setNewName("");
    await openFile(path);
    if (!dirRef.current) setDirty((d) => new Set(d).add(path));
  }

  function startCreate(): void {
    setNewName("src/");
    setCreating(true);
  }

  function requestClose(): void {
    if (
      dirty.size > 0 &&
      typeof window !== "undefined" &&
      !window.confirm("You have unsaved code edits. Close the code editor anyway?")
    ) {
      return;
    }
    onClose?.();
  }

  return (
    <div className="decode-root" role="region" aria-label="Code editor">
      <div className="decode-side">
        <div className="decode-side-head">
          <span className="decode-title">Files</span>
          <span className={`decode-src decode-src-${source}`}>
            {source === "disk"
              ? "project folder"
              : persist
                ? "draft (this browser)"
                : "unsaved (in-memory)"}
          </span>
          <button
            type="button"
            className="decode-new"
            aria-label="New file"
            title="New file"
            onClick={startCreate}
          >
            +
          </button>
        </div>
        {creating ? (
          <div className="decode-newrow">
            <input
              className="decode-newinput"
              autoFocus
              value={newName}
              placeholder="src/newfile.ts"
              aria-label="New file path"
              spellCheck={false}
              onChange={(e) => setNewName(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") {
                  e.preventDefault();
                  createFile(newName);
                } else if (e.key === "Escape") {
                  e.preventDefault();
                  setCreating(false);
                  setNewName("");
                }
              }}
              onBlur={() => {
                if (!newName.trim() || newName.trim().endsWith("/")) {
                  setCreating(false);
                  setNewName("");
                }
              }}
            />
          </div>
        ) : null}
        <div className="decode-tree">
          {tree.map((n) => (
            <TreeNode key={n.path} node={n} depth={0} openPath={openPath} dirty={dirty} onOpen={openFile} />
          ))}
        </div>
        <div className="decode-side-foot">
          {typesLoaded > 0 ? `TS ready · ${typesLoaded} SDK libs` : "TS ready"}
        </div>
      </div>
      <div className="decode-main">
        <div className="decode-bar">
          <span className="decode-path">
            {openPath || "—"}
            {openPath && dirty.has(openPath) ? <span className="decode-dirty"> ●</span> : null}
          </span>
          <span className="decode-lang">{openPath ? languageFor(openPath) : ""}</span>
          <button type="button" className="decode-btn" onClick={saveCurrent} disabled={!openPath}>
            Save{source === "virtual" ? (persist ? " (draft)" : " (memory)") : ""}
          </button>
          <button type="button" className="decode-btn" onClick={requestClose}>
            Close
          </button>
        </div>
        <div className="decode-editor" ref={hostRef} />
        {status ? <div className="decode-status">{status}</div> : null}
      </div>
    </div>
  );
}
