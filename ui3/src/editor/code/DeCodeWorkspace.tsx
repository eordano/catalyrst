import type { IStandaloneCodeEditor } from "monaco-editor";
import { useEffect, useMemo, useRef, useState } from "react";
import { loadMonaco, modelFor, languageFor } from "./monaco-host";
import type { DeWorkspaceCode } from "../types";
import { folderProject } from "../folder-project";
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
    if (SKIP_DIRS.has(entry.name) || entry.name.startsWith(".")) continue;
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
          <span className="decode-caret">{collapsed ? "\u{25B8}" : "\u{25BE}"}</span> {node.name}
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
      {dirty.has(node.path) ? <span className="decode-dirty">&#x25CF;</span> : null}
    </button>
  );
}

export interface DeCodeWorkspaceProps {
  code?: DeWorkspaceCode;
  store?: Map<string, string>;
  onClose?: () => void;
}

export default function DeCodeWorkspace({ code = {}, store, onClose }: DeCodeWorkspaceProps) {
  const { typesUrl = null, virtualFiles = [], getDir = null, hydrate = null, persist = null, project: sharedProject = null } = code;
  const project = useMemo(() => sharedProject?.createFileSession?.() ?? sharedProject, [sharedProject]);
  const hostRef = useRef<HTMLDivElement | null>(null);
  const editorRef = useRef<IStandaloneCodeEditor | null>(null);
  const monacoRef = useRef<Monaco | null>(null);
  const dirRef = useRef<FileSystemDirectoryHandle | null>(null);
  const folderSessionRef = useRef<NonNullable<DeWorkspaceCode["project"]> | null>(null);
  const [folderNamespace] = useState(() => crypto.randomUUID());
  const virtualsRef = useRef<Map<string, string>>(store ?? new Map());
  const [status, setStatus] = useState<string | null>("Loading editor\u{2026}");
  const [tree, setTree] = useState<FileTreeNode[]>([]);
  const [source, setSource] = useState<"disk" | "virtual" | "sdk">("virtual");
  const loadingFile = useRef(false);
  const projectNamespace = project ? Array.from(new TextEncoder().encode(project.id), (byte) => byte.toString(16).padStart(2, "0")).join("") : null;
  const modelPath = (path: string) => projectNamespace ? `sdk/${projectNamespace}/${path}` : folderSessionRef.current ? `folder/${folderNamespace}/${path}` : path;
  const [openPath, setOpenPath] = useState<string | null>(null);
  const [dirty, setDirty] = useState<Set<string>>(new Set());
  const [creating, setCreating] = useState(false);
  const [newName, setNewName] = useState("");
  const [typesLoaded, setTypesLoaded] = useState(0);
  const openPathRef = useRef<string | null>(null);
  const openSequence = useRef(0);
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
          if (p && !loadingFile.current) setDirty((d) => (d.has(p) ? d : new Set(d).add(p)));
        });

        if (project) {
          const paths = await project.list();
          if (dead) return;
          setSource("sdk");
          setTree(virtualTree(paths.map((path) => ({ path, text: "" }))));
          setStatus(null);
          const first = paths.includes("src/index.ts") ? "src/index.ts" : paths[0];
          if (first) await openFile(first);
          return;
        }

        let dir: FileSystemDirectoryHandle | null = null;
        try {
          dir = typeof getDir === "function" ? await getDir() : null;
        } catch {
          dir = null;
        }
        if (dir) {
          dirRef.current = dir;
          folderSessionRef.current = folderProject(dir);
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
      openSequence.current++;
      try {
        editorRef.current?.dispose();
      } catch {
      }
    };
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  async function openFile(path: string, reload = false) {
    const fileProject = project ?? folderSessionRef.current;
    const monaco = monacoRef.current;
    const editor = editorRef.current;
    if (!monaco || !editor) return;
    const sequence = ++openSequence.current;
    const previousModel = monaco.editor.getModel(monaco.Uri.parse(`file:///${modelPath(path)}`));
    if (fileProject && dirty.has(path) && previousModel && !reload) {
      loadingFile.current = true;
      editor.setModel(previousModel);
      loadingFile.current = false;
      setOpenPath(path);
      setStatus(null);
      return;
    }
    const previousPath = openPathRef.current;
    const activeModel = previousPath ? monaco.editor.getModel(monaco.Uri.parse(`file:///${modelPath(previousPath)}`)) : null;
    if (project || dirRef.current) {
      loadingFile.current = true;
      editor.setModel(null);
      setOpenPath(null);
      setStatus(`Opening ${path}\u2026`);
    }
    const failed = (error: unknown) => {
      if (sequence !== openSequence.current) return;
      editor.setModel(activeModel);
      loadingFile.current = false;
      setOpenPath(previousPath);
      setStatus(`Open failed: ${error instanceof Error ? error.message : String(error)}`);
    };
    let text: string | null | undefined = null;
    if (fileProject) {
      try { text = await fileProject.read(path); }
      catch (error) {
        failed(error);
        return;
      }
    } else {
      text = virtualsRef.current.get(path);
      if (text === undefined) return;
    }
    if (sequence !== openSequence.current) return;
    loadingFile.current = true;
    const model = modelFor(monaco, modelPath(path), text);
    if (project && model.getValue() !== text) model.setValue(text);
    editor.setModel(model);
    loadingFile.current = false;
    setOpenPath(path);
    setDirty((previous) => { const next = new Set(previous); next.delete(path); return next; });
    setStatus(null);
  }

  async function saveCurrent(): Promise<void> {
    const fileProject = project ?? folderSessionRef.current;
    const p = openPathRef.current;
    const monaco = monacoRef.current;
    if (!p || !monaco) return;
    const uri = monaco.Uri.parse(`file:///${modelPath(p)}`);
    const model = monaco.editor.getModel(uri);
    if (!model) return;
    const text = model.getValue();
    if (fileProject) {
      try { await fileProject.write(p, text); }
      catch (error) {
        setStatus(`Save failed: ${error instanceof Error ? error.message : String(error)}`);
        return;
      }
      setStatus(project ? "Saved to project. The SDK rebuilds the preview when watching is enabled." : "Saved to project folder.");
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
      if (model.getValue() === text) nd.delete(p);
      return nd;
    });
  }

  async function createFile(rawPath: string): Promise<void> {
    const path = rawPath.trim().replace(/^\/+/, "").replace(/\/+$/, "");
    if (!path) return;
    if (project) {
      try {
        await project.write(path, starterFor(path));
        setTree(virtualTree((await project.list()).map((path) => ({ path, text: "" }))));
      } catch (error) {
        setStatus(`Create failed: ${error instanceof Error ? error.message : String(error)}`);
        return;
      }
    } else if (dirRef.current) {
      try {
        await folderSessionRef.current!.write(path, starterFor(path));
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
    if (!dirRef.current && !project) setDirty((d) => new Set(d).add(path));
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
            {source === "sdk" ? "SDK project" : source === "disk"
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
          {typesLoaded > 0 ? `TS ready \u{B7} ${typesLoaded} SDK libs` : "TS ready"}
        </div>
      </div>
      <div className="decode-main">
        <div className="decode-bar">
          <span className="decode-path">
            {openPath || "\u{2014}"}
            {openPath && dirty.has(openPath) ? <span className="decode-dirty"> &#x25CF;</span> : null}
          </span>
          <span className="decode-lang">{openPath ? languageFor(openPath) : ""}</span>
          <button type="button" className="decode-btn" onClick={saveCurrent} disabled={!openPath}>
            Save{source === "virtual" ? (persist ? " (draft)" : " (memory)") : ""}
          </button>
          {project && <button type="button" className="decode-btn" disabled={!openPath} onClick={() => {
            if (openPath && (!dirty.has(openPath) || window.confirm("Discard unsaved code edits and reload this file from the project?"))) void openFile(openPath, true);
          }}>Reload file</button>}
          <button type="button" className="decode-btn" onClick={requestClose}>
            Close
          </button>
        </div>
        <div className="decode-editor" ref={hostRef} />
        {status ? <div className="decode-status" role={status.includes("failed:") ? "alert" : "status"}>{status}</div> : null}
      </div>
    </div>
  );
}
