import type { ComponentProps } from "react";

import ChModalCreateProject from "../components/ChModalCreateProject";
import ChTemplates from "../pages/ChTemplates";
import Spinner from "../../atoms/Spinner";
import Button from "../../atoms/Button";
import "./createprojectview.css";

type CpScaffoldFile = { name: string; note: string };
type CpScaffoldResult = {
  files: CpScaffoldFile[];
  written?: boolean;
  via?: "directory" | "download" | "canceled";
  folder?: string;
};

const SCAFFOLD_ERROR_HINTS: { code: RegExp; message: string }[] = [
  { code: /ENOENT/, message: "Folder not found \u{2014} it may have been moved or deleted." },
  { code: /EACCES|NotAllowedError|SecurityError/, message: "Permission denied \u{2014} grant write access to the folder and try again." },
  { code: /EEXIST/, message: "A file or folder with that name already exists." },
  { code: /ENOSPC/, message: "Not enough disk space to write the project." },
  { code: /AbortError|canceled|cancelled/, message: "Scene creation was cancelled \u{2014} no folder was chosen." },
];

function humanizeScaffoldError(error?: string): string {
  if (!error) return "unknown error";
  const hit = SCAFFOLD_ERROR_HINTS.find((h) => h.code.test(error));
  return hit ? hit.message : error;
}

const FOLDER_RECOVERABLE =
  /EACCES|ENOSPC|NotAllowedError|SecurityError|AbortError|canceled|cancelled/;
function isFolderRecoverable(error?: string): boolean {
  return !!error && FOLDER_RECOVERABLE.test(error);
}

type CreateProjectViewProps = {
  view?: string;
  step?: string;
  name?: string;
  takenPaths?: string[];
  pathError?: string;
  projectSlug?: string;
  originFrom?: string;
  signedIn?: boolean;
  submitting?: boolean;
  folderCancelled?: boolean;
  downloadFallback?: boolean;
  result?: CpScaffoldResult;
  error?: string;
  onSubmitDetails?: (next?: { name: string; path: string }) => void;
  onDetailsChange?: (next: { name: string; path: string }) => void;
  onClose?: () => void;
  onPickTemplate?: ComponentProps<typeof ChTemplates>["onSelectTemplate"];
  onBack?: () => void;
  onRetry?: () => void;
  onChooseFolder?: () => void;
};

export default function CreateProjectView({
  view = "naming",
  step = "name",
  name = "",
  takenPaths = [],
  pathError = undefined,
  projectSlug = "",
  originFrom = undefined,
  signedIn = false,
  submitting = false,
  folderCancelled = false,
  downloadFallback = false,
  result = undefined,
  error = undefined,
  onSubmitDetails = undefined,
  onDetailsChange = undefined,
  onClose = undefined,
  onPickTemplate = undefined,
  onBack = undefined,
  onRetry = undefined,
  onChooseFolder = undefined,
}: CreateProjectViewProps) {
  return (
    <div className="create-project-wizard" data-step={step}>
      {view === "naming" && (
        <ChModalCreateProject
          open
          initialValue={{ name }}
          takenPaths={takenPaths}
          error={pathError}
          submitting={submitting}
          onSubmit={onSubmitDetails}
          onChange={onDetailsChange}
          onClose={onClose}
        />
      )}

      {view === "templating" && (
        <>
          {folderCancelled && (
            <p className="create-project-wizard__note" role="note">
              Folder selection was cancelled &#x2014; pick a template again, then choose
              a folder to save your scene.
            </p>
          )}
          <ChTemplates
            embedded
            onBack={onBack}
            onSelectTemplate={onPickTemplate}
          />
        </>
      )}

      {view === "scaffolding" && (
        <div className="create-project-wizard__scaffold">
          <Spinner />
          <p>Scaffolding your scene&#x2026;</p>
          {downloadFallback && (
            <p className="create-project-wizard__note" role="note">
              This browser can&apos;t write to a folder directly &#x2014; your scene
              files will download as a zip instead.
            </p>
          )}
        </div>
      )}

      {view === "created" && (
        <div className="create-project-wizard__created-banner" role="status">
          <p className="create-project-wizard__created-title">
            Scene created &#x2014; {name}
          </p>
          <p className="create-project-wizard__created-sub">
            {!result ? (
              <>Your scene is ready. Open it in the web editor to start building.</>
            ) : result.via === "download" ? (
              <>
                This browser can&apos;t write to a folder directly, so{" "}
                <strong>{result.folder}.zip</strong> (
                {result.files.length} files) was downloaded instead. Unzip it,
                then run <code>npm install &amp;&amp; npm start</code> inside
                the folder to preview locally &#x2014; or open the scene in the web
                editor to start building.
              </>
            ) : (
              <>
                Wrote{" "}
                <strong>{result.files.length} files</strong> to{" "}
                <strong title={result.folder}>{result.folder}/</strong> on your disk
                (including scene.json + main.composite). Open it in the web editor
                to start building.
              </>
            )}
          </p>
          {!signedIn && (
            <p className="create-project-wizard__created-sub" role="note">
              Heads up: publishing this scene later needs a connected wallet. You
              can build now and sign in when you&apos;re ready to publish.
            </p>
          )}
          <div className="create-project-wizard__controls">
            <a
              className="create-project-wizard__btn create-project-wizard__btn--primary"
              href={
                projectSlug
                  ? `/creator-hub/scene-editor?source=local&project=${encodeURIComponent(projectSlug)}&from=${encodeURIComponent(originFrom || "scenes")}`
                  : `/creator-hub/scene-editor?source=local&from=${encodeURIComponent(originFrom || "scenes")}`
              }
            >
              Open in editor
            </a>
            <a className="create-project-wizard__btn" href="/create/scenes">
              Go to My Scenes
            </a>
            {result?.via === "download" && (
              <a
                className="create-project-wizard__btn"
                href="/landings/creator-hub-download"
              >
                Get the desktop app
              </a>
            )}
          </div>
        </div>
      )}

      {view === "error" && (
        <div className="create-project-wizard__scaffold">
          <div className="create-project-wizard__controls">
            <p className="create-project-wizard__error" role="alert">
              Scaffolding failed: {humanizeScaffoldError(error)}
            </p>
            {isFolderRecoverable(error) && (
              <Button variant="primary" onClick={onChooseFolder}>
                Choose folder again
              </Button>
            )}
            <Button
              variant={isFolderRecoverable(error) ? "secondary" : "primary"}
              onClick={onRetry}
            >
              Retry
            </Button>
          </div>
        </div>
      )}
    </div>
  );
}
