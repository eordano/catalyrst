
type Monaco = typeof import("monaco-editor");
type ITextModel = import("monaco-editor").ITextModel;

interface MonacoBoot {
  monaco: Monaco;
  typesLoaded: number;
}

let monacoPromise: Promise<MonacoBoot> | null = null;

export function loadMonaco(typesUrl?: string): Promise<MonacoBoot> {
  if (!monacoPromise) {
    monacoPromise = boot(typesUrl).catch((e) => {
      monacoPromise = null;
      throw e;
    });
  }
  return monacoPromise;
}

async function boot(typesUrl?: string): Promise<MonacoBoot> {
  const [monaco, { default: editorWorkerUrl }, { default: tsWorkerUrl }] = await Promise.all([
    import("monaco-editor"),
    import("monaco-editor/esm/vs/editor/editor.worker?worker&url"),
    import("monaco-editor/esm/vs/language/typescript/ts.worker?worker&url"),
  ]);

  const blobWorker = (url: string | undefined, name: string): Worker => {
    if (!url || typeof url !== "string") {
      throw new Error(`[code] ${name} worker asset missing from this build`);
    }
    const abs = new URL(url, self.location.href).href;
    const blob = new Blob([`import ${JSON.stringify(abs)};`], {
      type: "text/javascript",
    });
    return new Worker(URL.createObjectURL(blob), { type: "module", name });
  };

  self.MonacoEnvironment = {
    getWorker(_id, label) {
      if (label === "typescript" || label === "javascript") {
        return blobWorker(tsWorkerUrl, "ts");
      }
      return blobWorker(editorWorkerUrl, "editor");
    },
  };

  const ts = monaco.languages.typescript;
  const compilerOptions: import("monaco-editor").languages.typescript.CompilerOptions = {
    target: ts.ScriptTarget.ES2020,
    module: ts.ModuleKind.ESNext,
    moduleResolution: ts.ModuleResolutionKind.NodeJs,
    jsx: ts.JsxEmit.ReactJSX,
    strict: true,
    noEmit: true,
    esModuleInterop: true,
    allowNonTsExtensions: true,
    allowJs: true,
    baseUrl: "file:///",
  };
  ts.typescriptDefaults.setCompilerOptions(compilerOptions);
  ts.javascriptDefaults.setCompilerOptions(compilerOptions);
  ts.typescriptDefaults.setEagerModelSync(true);

  let typesLoaded = 0;
  if (typesUrl) {
    try {
      const res = await fetch(typesUrl, { credentials: "omit" });
      if (res.ok) {
        const bundle = (await res.json()) as { files?: Record<string, string> } | null;
        const files: Record<string, string> = bundle && bundle.files ? bundle.files : {};
        for (const [path, content] of Object.entries(files)) {
          ts.typescriptDefaults.addExtraLib(String(content), `file:///node_modules/${path}`);
          typesLoaded++;
        }
      }
    } catch (e) {
      console.warn("[code] DCL SDK types unavailable:", e);
    }
  }

  try {
    self.__dclMonaco = monaco;
  } catch {
  }

  return { monaco, typesLoaded };
}

const LANG_BY_EXT: Record<string, string> = {
  ts: "typescript",
  tsx: "typescript",
  js: "javascript",
  jsx: "javascript",
  json: "json",
  md: "markdown",
  css: "css",
  html: "html",
  crdt: "plaintext",
  composite: "json",
};

export function languageFor(path: string): string {
  const ext = String(path).split(".").pop()?.toLowerCase() ?? "";
  return LANG_BY_EXT[ext] || "plaintext";
}

export function modelFor(monaco: Monaco, path: string, content: string): ITextModel {
  const uri = monaco.Uri.parse(`file:///${String(path).replace(/^\/+/, "")}`);
  const existing = monaco.editor.getModel(uri);
  if (existing) return existing;
  return monaco.editor.createModel(content, languageFor(path), uri);
}
