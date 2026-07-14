declare module "monaco-editor" {
  export interface Uri {
    toString(): string;
  }
  export namespace Uri {
    function parse(value: string): Uri;
  }

  export interface ITextModel {
    readonly uri: Uri;
    getValue(): string;
    dispose(): void;
  }

  export interface IDisposable {
    dispose(): void;
  }

  export interface IStandaloneEditorConstructionOptions {
    theme?: string;
    automaticLayout?: boolean;
    minimap?: { enabled?: boolean };
    fontSize?: number;
    scrollBeyondLastLine?: boolean;
    fixedOverflowWidgets?: boolean;
    [option: string]: unknown;
  }

  export interface IStandaloneCodeEditor {
    addCommand(keybinding: number, handler: () => void): void;
    onDidChangeModelContent(listener: () => void): IDisposable;
    setModel(model: ITextModel | null): void;
    dispose(): void;
  }

  export namespace editor {
    function getModel(uri: Uri): ITextModel | null;
    function createModel(value: string, language?: string, uri?: Uri): ITextModel;
    function create(
      container: HTMLElement,
      options?: IStandaloneEditorConstructionOptions,
    ): IStandaloneCodeEditor;
  }

  export enum KeyMod {
    CtrlCmd,
    Shift,
    Alt,
    WinCtrl,
  }
  export enum KeyCode {
    KeyS,
  }

  export namespace languages {
    export namespace typescript {
      export enum ScriptTarget {
        ES3,
        ES5,
        ES2015,
        ES2016,
        ES2017,
        ES2018,
        ES2019,
        ES2020,
        ESNext,
        Latest,
      }
      export enum ModuleKind {
        None,
        CommonJS,
        AMD,
        UMD,
        System,
        ES2015,
        ESNext,
      }
      export enum ModuleResolutionKind {
        Classic,
        NodeJs,
      }
      export enum JsxEmit {
        None,
        Preserve,
        React,
        ReactNative,
        ReactJSX,
        ReactJSXDev,
      }
      export interface CompilerOptions {
        [option: string]: unknown;
      }
      export interface IDisposable {
        dispose(): void;
      }
      export interface LanguageServiceDefaults {
        setCompilerOptions(options: CompilerOptions): void;
        setEagerModelSync(value: boolean): void;
        addExtraLib(content: string, filePath?: string): IDisposable;
      }
      export const typescriptDefaults: LanguageServiceDefaults;
      export const javascriptDefaults: LanguageServiceDefaults;
    }
  }
}

declare module "monaco-editor/esm/vs/editor/editor.worker?worker&url" {
  const url: string;
  export default url;
}
declare module "monaco-editor/esm/vs/language/typescript/ts.worker?worker&url" {
  const url: string;
  export default url;
}

interface Window {
  MonacoEnvironment?: {
    getWorker(workerId: string, label: string): Worker;
  };
  __dclMonaco?: typeof import("monaco-editor");
}
