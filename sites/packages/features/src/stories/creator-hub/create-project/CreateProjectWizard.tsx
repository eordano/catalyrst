import { useCallback, useEffect, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useNavigate, useSearchParams } from "react-router";

import CreateProjectView from "@ui/creatorhub/workflows/CreateProjectView";
import { resolveBreadcrumbOrigin } from "../../../components/creator-hub/breadcrumbOrigins";

import type { TrackContext } from "@core/lib/telemetry/track";
import {
  buildScaffoldFiles,
  writeScaffoldFiles,
  COMPOSITE_FILENAME,
} from "@data/lib/fs/scaffold-project";
import {
  openDirectory,
  readDirectoryFiles,
  supportsDirectoryPicker,
} from "@data/lib/fs/disk";
import { handleStore } from "@data/lib/fs/handle-store";
import { useAuth } from "@data/lib/auth/index";
import {
  createProjectMachine,
  resolveCreateProjectSnapshot,
  slugToState,
  slugifyProjectName,
  stateToSlug,
  type ScaffoldFn,
  type ScaffoldResult,
  type TrackFn,
} from "./machine";

export type TemplateOption = {
  id: string;
  title: string;
  description: string;
  tags: string[];
  difficulty: string;
  default: boolean;
  github_link: string;
};

export type CreateProjectWizardProps = {
  trackCtx: TrackContext;
  defaults: { name: string; path: string };
  takenPaths: string[];
  templates: TemplateOption[];
  initialStep?: string;
  initialTemplate?: string;
  initialName?: string;
  originFrom?: string;
  scaffold?: ScaffoldFn;
  track?: TrackFn;
};

function makeDiskScaffold(
  templates: TemplateOption[],
  getDir?: () => FileSystemDirectoryHandle | null,
): ScaffoldFn {
  return async ({ name, template }): Promise<ScaffoldResult> => {
    const dir = getDir?.() ?? undefined;
    if (!dir && supportsDirectoryPicker()) {
      throw new Error("Scene creation canceled \u{2014} no folder was chosen.");
    }
    const tpl = templates.find((t) => t.id === template);
    const files = buildScaffoldFiles({
      name,
      template,
      templateTitle: tpl?.title,
      githubLink: tpl?.github_link,
    });
    const result = await writeScaffoldFiles(files, {
      name,
      dir,
    });
    if (result.via === "canceled") {
      throw new Error("Scene creation canceled \u{2014} no folder was chosen.");
    }
    return {
      files: files.map((f) => ({ name: f.path, note: "written to disk" })),
      written: result.written,
      via: result.via,
      folder: result.folder,
      dir: result.dir ?? null,
    };
  };
}

async function scanProjectFolders(
  dir: FileSystemDirectoryHandle,
): Promise<string[]> {
  try {
    const files = await readDirectoryFiles(dir);
    const dirs = new Set<string>();
    for (const key of Object.keys(files)) {
      const slash = key.indexOf("/");
      if (slash > 0) dirs.add(key.slice(0, slash));
    }
    return Array.from(dirs);
  } catch {
    return [];
  }
}

export default function CreateProjectWizard(props: CreateProjectWizardProps) {
  const [searchParams] = useSearchParams();
  const [epoch, setEpoch] = useState(0);

  const urlStep =
    (searchParams.get("step")?.trim() || props.initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  const templateId =
    (searchParams.get("template")?.trim() || props.initialTemplate) || undefined;

  const urlName =
    (searchParams.get("name")?.trim() || props.initialName) || undefined;

  const onResync = useCallback(() => setEpoch((e) => e + 1), []);

  return (
    <CreateProjectWizardInner
      key={epoch}
      stateId={stateId}
      templateId={templateId}
      urlName={urlName}
      onResync={onResync}
      {...props}
    />
  );
}

type InnerProps = CreateProjectWizardProps & {
  stateId: ReturnType<typeof slugToState>;
  templateId?: string;
  urlName?: string;
  onResync: () => void;
};

function CreateProjectWizardInner({
  stateId,
  templateId,
  urlName,
  onResync,
  trackCtx,
  defaults,
  takenPaths,
  templates,
  originFrom,
  scaffold,
  track,
}: InnerProps) {
  const [searchParams, setSearchParams] = useSearchParams();
  const navigate = useNavigate();

  const dirHandleRef = useRef<FileSystemDirectoryHandle | null>(null);
  const realScaffold = useRef(
    scaffold ?? makeDiskScaffold(templates, () => dirHandleRef.current),
  ).current;

  const [folderCancelled, setFolderCancelled] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const submitGate = useRef(false);

  const [scannedPaths, setScannedPaths] = useState<string[]>([]);
  const effectiveTaken = scannedPaths.length
    ? Array.from(new Set([...takenPaths, ...scannedPaths]))
    : takenPaths;

  const initialName = urlName ?? defaults.name;

  const snapshot = useRef(
    resolveCreateProjectSnapshot({
      step: stateId,
      trackCtx,
      scaffold: realScaffold,
      track,
      name: initialName,
      path: defaults.path,
      template: templateId,
    }),
  ).current;

  const [state, send] = useMachine(createProjectMachine, {
    input: {
      trackCtx,
      scaffold: realScaffold,
      track,
      template: templateId,
      name: initialName,
      path: defaults.path,
    },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);

  const { isConnected } = useAuth();

  const closeWizard = () =>
    navigate(resolveBreadcrumbOrigin(searchParams.get("from")?.trim() || null).to);

  const pickFolder = async (): Promise<{
    cancelled: boolean;
    existing: string[] | null;
  }> => {
    let handle: FileSystemDirectoryHandle | null = null;
    try {
      handle = await openDirectory("readwrite");
    } catch {
      handle = null;
    }
    dirHandleRef.current = handle;
    const cancelled = supportsDirectoryPicker() && !handle;
    setFolderCancelled(cancelled);
    if (!handle) return { cancelled, existing: null };
    const existing = await scanProjectFolders(handle);
    setScannedPaths(existing);
    return { cancelled: false, existing };
  };

  const submitDetails = async (next?: { name: string; path?: string }) => {
    if (!next || submitGate.current) return;
    submitGate.current = true;
    setSubmitting(true);
    try {
      clearTimeout(draftTimer.current);
      if (!next.name.trim()) {
        send({
          type: "SET_NAME",
          name: next.name,
          valid: false,
          error: "Scene name is required.",
        });
        return;
      }
      const slug = slugifyProjectName(next.name);
      let taken = effectiveTaken;
      if (templateId && !scaffold) {
        const picked = await pickFolder();
        if (picked.cancelled) {
          send({
            type: "SET_NAME",
            name: next.name,
            valid: false,
            error:
              "Folder selection was cancelled \u{2014} click Create and pick a folder to save your scene.",
          });
          return;
        }
        if (picked.existing)
          taken = Array.from(new Set([...taken, ...picked.existing]));
      }
      const valid = !taken.includes(slug);
      send({
        type: "SET_NAME",
        name: next.name,
        valid,
        error: valid
          ? undefined
          : "A scene with that name already exists.",
      });
    } finally {
      submitGate.current = false;
      setSubmitting(false);
    }
  };

  useEffect(() => {
    if (!state.matches("created")) return;
    const slug = state.context.projectSlug;
    if (!slug) return;
    const meta: Parameters<typeof handleStore.putMeta>[1] = {
      title: state.context.name || slug,
      base: "0,0",
    };
    const template = state.context.template;
    if (template && template !== "empty") {
      const tpl = templates.find((t) => t.id === template);
      const compositeFile = buildScaffoldFiles({
        name: state.context.name,
        template,
        templateTitle: tpl?.title,
        githubLink: tpl?.github_link,
      }).find((f) => f.path === COMPOSITE_FILENAME);
      if (compositeFile) meta.composite = compositeFile.text;
    }
    handleStore.putMeta(slug, meta).catch(() => {});
    const projectDir = state.context.result?.dir;
    if (projectDir && state.context.result?.via === "directory") {
      handleStore.put(slug, projectDir).catch(() => {});
    }
  }, [state, templates]);

  const urlSlug = searchParams.get("step")?.trim() || null;
  const syncedSlug = useRef<string | null>(urlSlug);
  const pendingNav = useRef<string | null>(null);
  const latest = useRef({ name: state.context.name, setSearchParams, onResync });
  latest.current = { name: state.context.name, setSearchParams, onResync };
  useEffect(() => {
    if (step === "scaffold" || step === "created" || step === "error") return;
    if (urlSlug === step) {
      syncedSlug.current = step;
      pendingNav.current = null;
      return;
    }
    if (pendingNav.current === step) return;
    if (syncedSlug.current === urlSlug) {
      const from = syncedSlug.current;
      const push = (from === "name" || from === "template") && step === "template";
      syncedSlug.current = step;
      pendingNav.current = step;
      const committedName = latest.current.name;
      latest.current.setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("step", step);
          if (step !== "name" || params.has("name")) {
            params.set("name", committedName);
          }
          return params;
        },
        { replace: !push, preventScrollReset: true },
      );
      return;
    }
    syncedSlug.current = urlSlug;
    pendingNav.current = null;
    if (slugToState(urlSlug) === "naming" && step === "template") {
      send({ type: "BACK" });
      const committedName = latest.current.name;
      latest.current.setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          params.set("name", committedName);
          return params;
        },
        { replace: true, preventScrollReset: true },
      );
    } else {
      latest.current.onResync();
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [urlSlug, step, send]);

  const draftTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(draftTimer.current), []);
  const persistNameDraft = (next: { name: string }) => {
    clearTimeout(draftTimer.current);
    draftTimer.current = setTimeout(() => {
      setSearchParams(
        (prev) => {
          const params = new URLSearchParams(prev);
          const trimmed = next.name.trim();
          if (trimmed) params.set("name", next.name);
          else params.delete("name");
          return params;
        },
        { replace: true, preventScrollReset: true },
      );
    }, 250);
  };

  return (
    <CreateProjectView
      view={value}
      step={step}
      name={state.context.name}
      takenPaths={effectiveTaken}
      pathError={state.context.pathError}
      projectSlug={state.context.projectSlug}
      originFrom={originFrom}
      signedIn={isConnected}
      result={state.context.result}
      error={state.context.error}
      onSubmitDetails={submitDetails}
      onDetailsChange={persistNameDraft}
      onClose={closeWizard}
      submitting={submitting}
      folderCancelled={folderCancelled}
      downloadFallback={!supportsDirectoryPicker() || folderCancelled}
      onPickTemplate={async (tpl?: { id: string } | null) => {
        if (!scaffold) {
          const picked = await pickFolder();
          if (picked.cancelled) return;
          if (picked.existing) {
            const slug = slugifyProjectName(state.context.name);
            if (picked.existing.includes(slug)) {
              send({ type: "BACK" });
              send({
                type: "SET_NAME",
                name: state.context.name,
                valid: false,
                error: "A scene with that name already exists in that folder.",
              });
              return;
            }
          }
        }
        send({ type: "SELECT_TEMPLATE", template: tpl?.id ?? "empty" });
      }}
      onBack={() => send({ type: "BACK" })}
      onRetry={async () => {
        if (!scaffold) {
          const picked = await pickFolder();
          if (picked.cancelled) return;
        }
        send({ type: "RETRY" });
      }}
      onChooseFolder={async () => {
        if (!scaffold) {
          const picked = await pickFolder();
          if (picked.cancelled) return;
        }
        send({ type: "CHOOSE_FOLDER" });
      }}
    />
  );
}
