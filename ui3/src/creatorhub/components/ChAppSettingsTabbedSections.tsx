import { useCallback, useId, useMemo, useState } from "react";
import type { ReactNode } from "react";
import { asset } from "../../asset";
import Button from "../../atoms/Button";
import Checkbox from "../../atoms/Checkbox";
import { CheckFill } from "../../atoms/icons";
import EmptyState from "../../components/EmptyState";
import ChDialogShell from "./ChDialogShell";
import "./chappsettingstabbedsections.css";

type PreviewOptions = {
  debugger: boolean;
  enableLandscapeTerrains: boolean;
  showWarnings: boolean;
};

type AppSettings = {
  scenesPath: string;
  dependencyUpdateStrategy: string;
  previewOptions: PreviewOptions;
};

type CodeEditor = {
  name: string;
  path: string;
  isDefault?: boolean;
};

type UpdateState =
  | "idle"
  | "checking"
  | "downloading"
  | "available"
  | "downloaded"
  | "up_to_date";

type ReleaseNotes = {
  version?: string;
  content: string[];
};

const DOWNLOAD_HREF = "/landings/creator-hub-download";
const RELEASES_HREF = "https://github.com/decentraland/creator-hub/releases";

const COPY = {
  title: "App Preferences",
  tabs: [
    { value: "scenes", label: "Scenes" },
    { value: "editor", label: "Editor" },
    { value: "about", label: "About" },
  ],
  scenes: {
    folderLabel: "Scenes Folder",
    resetButton: "Reset",
    invalidPath:
      "Folder doesn't exist or can't save files there. Please choose a different folder.",
    dependenciesLabel: "Scene Dependencies",
    dependencies: [
      { value: "auto_update", label: "Auto-update all my scenes" },
      { value: "notify", label: "Notify me of new version updates" },
      { value: "do_nothing", label: "Do nothing" },
    ],
  },
  editor: {
    codeEditorLabel: "Code editor of choice",
    chooseDevice: "Choose from your device...",
    selectPlaceholder: "Add or select a default editor",
    previewOptionsLabel: "Preview Options",
    debugger: "Open Debug Console",
    landscapeTerrain: "Enable Landscape Terrains",
    appWarningsLabel: "App Warnings",
    showWarnings:
      "Show a warning message each time you preview or publish a scene that contains custom code",
  },
  about: {
    title: "Decentraland Creator Hub",
    viewChangelog: "View changelog",
  },
  web: {
    noticeTitle: "Manage in the desktop app",
    noticeBody:
      "Your scenes folder, default code editor, and automatic updates are handled by the Creator Hub desktop app.",
    download: "Download Creator Hub",
  },
  update: {
    check: "Check for updates",
    checking: "Loading...",
    upToDate: "Creator Hub is up to date.",
    available: "Update Available",
    update: "Update now",
    install: "Install now",
    downloading: (p: number) => `Downloading... ${p}%`,
    applying: "Applying updates.",
    dontClose: "Please don't close the app.",
    autoRestart: "Creator Hub will auto-restart after the update.",
  },
};

const SettingsIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    <path
      fill="currentColor"
      d="M19.14 12.94c.04-.3.06-.61.06-.94 0-.32-.02-.64-.07-.94l2.03-1.58c.18-.14.23-.41.12-.61l-1.92-3.32c-.12-.22-.37-.29-.59-.22l-2.39.96c-.5-.38-1.03-.7-1.62-.94l-.36-2.54c-.04-.24-.24-.41-.48-.41h-3.84c-.24 0-.43.17-.47.41l-.36 2.54c-.59.24-1.13.57-1.62.94l-2.39-.96c-.22-.08-.47 0-.59.22L2.74 8.87c-.12.21-.08.47.12.61l2.03 1.58c-.05.3-.09.63-.09.94s.02.64.07.94l-2.03 1.58c-.18.14-.23.41-.12.61l1.92 3.32c.12.22.37.29.59.22l2.39-.96c.5.38 1.03.7 1.62.94l.36 2.54c.05.24.24.41.48.41h3.84c.24 0 .44-.17.47-.41l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.22.08.47 0 .59-.22l1.92-3.32c.12-.22.07-.47-.12-.61l-2.01-1.58zM12 15.6c-1.98 0-3.6-1.62-3.6-3.6s1.62-3.6 3.6-3.6 3.6 1.62 3.6 3.6-1.62 3.6-3.6 3.6z"
    />
  </svg>
);

const FolderIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    <path
      fill="currentColor"
      d="M10 4H4c-1.1 0-1.99.9-1.99 2L2 18c0 1.1.9 2 2 2h16c1.1 0 2-.9 2-2V8c0-1.1-.9-2-2-2h-8l-2-2z"
    />
  </svg>
);

const DeleteIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    <path
      fill="currentColor"
      d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"
    />
  </svg>
);

const ArrowDropDownIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    <path fill="currentColor" d="M7 10l5 5 5-5z" />
  </svg>
);

const InfoOutlinedIcon = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true" focusable="false">
    <path
      fill="currentColor"
      d="M11 7h2v2h-2zm0 4h2v6h-2zm1-9C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8z"
    />
  </svg>
);

function Radio({ checked }: { checked: boolean }) {
  return (
    <span className={"chast__radio" + (checked ? " is-on" : "")}>
      <span className="chast__radio-dot" />
    </span>
  );
}

type ScenesTabProps = {
  settings: AppSettings;
  error: string | null;
  isCustomScenesPath: boolean;
  onChange: (value: string) => void;
  onReset: () => void;
  onOpenFolder: () => void;
};

function ScenesTab({ settings, error, isCustomScenesPath, onChange, onReset, onOpenFolder }: ScenesTabProps) {
  const uid = useId();
  const folderId = `${uid}-folder`;
  const depsLabelId = `${uid}-deps`;
  return (
    <div className="chast__form">
      <div className="chast__group chast__group--scenes">
        <label className="chast__label" htmlFor={folderId}>{COPY.scenes.folderLabel}</label>
        <div className="chast__folderrow">
          <div className={"chast__input" + (error ? " is-error" : "")}>
            <input
              id={folderId}
              className="chast__field"
              type="text"
              value={settings.scenesPath}
              onChange={(e) => onChange(e.target.value)}
            />
            <span className="chast__adornment">
              {isCustomScenesPath && (
                <button type="button" className="chast__textbtn" onClick={onReset}>
                  {COPY.scenes.resetButton}
                </button>
              )}
              <button
                type="button"
                className="chast__iconbtn"
                aria-label="choose folder"
                onClick={onOpenFolder}
              >
                <FolderIcon />
              </button>
            </span>
          </div>
        </div>
        {error && <p className="chast__error">{error}</p>}
      </div>

      <div className="chast__group chast__group--deps">
        <p className="chast__label" id={depsLabelId}>{COPY.scenes.dependenciesLabel}</p>
        <div className="chast__radiogroup" role="radiogroup" aria-labelledby={depsLabelId}>
          {COPY.scenes.dependencies.map((opt) => (
            <label key={opt.value} className="chast__option">
              <Radio checked={settings.dependencyUpdateStrategy === opt.value} />
              <span className="chast__optlabel">{opt.label}</span>
            </label>
          ))}
        </div>
      </div>
    </div>
  );
}

type EditorTabProps = {
  settings: AppSettings;
  editors: CodeEditor[];
  defaultEditor: string;
  onPreviewOptionChange: (key: keyof PreviewOptions, checked: boolean) => void;
};

function EditorTab({ settings, editors, defaultEditor, onPreviewOptionChange }: EditorTabProps) {
  const [menuOpen, setMenuOpen] = useState(false);
  const selectedName =
    editors.find((e) => e.path === defaultEditor)?.name ||
    COPY.editor.selectPlaceholder;

  return (
    <div className="chast__form">
      <div className="chast__group chast__group--editor">
        <p className="chast__label">{COPY.editor.codeEditorLabel}</p>
        <div className="chast__select-wrap">
          <button
            type="button"
            className="chast__select"
            aria-haspopup="listbox"
            aria-expanded={menuOpen}
            onClick={() => setMenuOpen((v) => !v)}
          >
            <span
              className={
                "chast__select-value" +
                (selectedName === COPY.editor.selectPlaceholder ? " is-placeholder" : "")
              }
            >
              {selectedName}
            </span>
            <span className="chast__select-arrow">
              <ArrowDropDownIcon />
            </span>
          </button>
          {menuOpen && (
            <ul className="chast__menu" role="listbox">
              {editors.map((editor) => (
                <li
                  key={editor.path}
                  className="chast__menuitem"
                  role="option"
                  aria-selected={editor.path === defaultEditor}
                >
                  <span className="chast__menuname">{editor.name}</span>
                  <span className="chast__menuactions">
                    {editor.path === defaultEditor && (
                      <span className="chast__defaulticon">
                        <CheckFill />
                      </span>
                    )}
                    <button
                      type="button"
                      className="chast__iconbtn chast__iconbtn--small"
                      aria-label="remove editor"
                    >
                      <DeleteIcon />
                    </button>
                  </span>
                </li>
              ))}
              <li className="chast__menuitem chast__menuitem--custom" role="option">
                {COPY.editor.chooseDevice}
              </li>
            </ul>
          )}
        </div>
      </div>

      <div className="chast__group chast__group--preview">
        <p className="chast__label">{COPY.editor.previewOptionsLabel}</p>
        <Checkbox
          checked={!!settings.previewOptions.debugger}
          onChange={(checked) => onPreviewOptionChange("debugger", checked)}
        >
          {COPY.editor.debugger}
        </Checkbox>
        <Checkbox
          checked={!!settings.previewOptions.enableLandscapeTerrains}
          onChange={(checked) => onPreviewOptionChange("enableLandscapeTerrains", checked)}
        >
          {COPY.editor.landscapeTerrain}
        </Checkbox>
      </div>

      <div className="chast__group chast__group--warnings">
        <p className="chast__label">{COPY.editor.appWarningsLabel}</p>
        <Checkbox
          checked={!!settings.previewOptions.showWarnings}
          onChange={(checked) => onPreviewOptionChange("showWarnings", checked)}
        >
          {COPY.editor.showWarnings}
        </Checkbox>
      </div>
    </div>
  );
}

function renderReleaseNotes(content: string[]): ReactNode[] {
  const out: ReactNode[] = [];
  let items: string[] = [];
  const flush = () => {
    if (!items.length) return;
    out.push(
      <ul key={`ul-${out.length}`} className="chast__md-ul">
        {items.map((item, j) => (
          <li key={j} className="chast__md-li">
            {item}
          </li>
        ))}
      </ul>,
    );
    items = [];
  };
  content.forEach((line, i) => {
    if (line.startsWith("- ")) {
      items.push(line.slice(2));
      return;
    }
    flush();
    out.push(
      line.startsWith("## ") ? (
        <h3 key={i} className="chast__md-h3">
          {line.slice(3)}
        </h3>
      ) : (
        <p key={i} className="chast__md-p">
          {line}
        </p>
      ),
    );
  });
  flush();
  return out;
}

type UpdateSettingsProps = {
  updateState: UpdateState;
  version?: string;
  releaseNotes?: ReleaseNotes | null;
  progress?: number;
};

function UpdateSettings({ updateState, version, releaseNotes, progress = 0 }: UpdateSettingsProps) {
  const button = useMemo(() => {
    switch (updateState) {
      case "checking":
        return { text: COPY.update.checking, disabled: true, tone: "secondary" };
      case "downloading":
        return {
          text: COPY.update.downloading(progress),
          disabled: true,
          tone: "inherit",
        };
      case "available":
        return { text: COPY.update.update, disabled: false, tone: "primary" };
      case "downloaded":
        return { text: COPY.update.install, disabled: false, tone: "primary" };
      case "up_to_date":
      case "idle":
      default:
        return { text: COPY.update.check, disabled: false, tone: "secondary" };
    }
  }, [updateState, progress]);

  const renderStatus = () => {
    if (updateState === "available" || updateState === "downloaded") {
      return (
        <>
          <h6 className="chast__release-header">
            {COPY.update.available}: v{releaseNotes?.version || version}
          </h6>
          {releaseNotes && releaseNotes.content.length > 0 && (
            <div className="chast__release-notes">
              <div className="chast__release-content">{renderReleaseNotes(releaseNotes.content)}</div>
            </div>
          )}
        </>
      );
    }
    if (updateState === "downloading") {
      return (
        <div className="chast__progress-container">
          <p className="chast__progress-line">{COPY.update.applying}</p>
          <p className="chast__progress-line">{COPY.update.dontClose}</p>
        </div>
      );
    }
    if (updateState === "up_to_date") {
      return <p className="chast__uptodate">{COPY.update.upToDate}</p>;
    }
    return null;
  };

  const canInstall = updateState === "downloaded";

  return (
    <div className="chast__update">
      <div className="chast__update-status">{renderStatus()}</div>
      <div className="chast__update-buttons">
        <Button
          variant={button.tone === "primary" ? "primary" : "secondary"}
          className="chast__btn--full"
          disabled={button.disabled}
        >
          {button.text}
        </Button>
        {canInstall && (
          <div className="chast__update-message">
            <span className="chast__update-message-icon">
              <InfoOutlinedIcon />
            </span>
            <span>{COPY.update.autoRestart}</span>
          </div>
        )}
      </div>
    </div>
  );
}

type AboutTabProps = {
  version?: string;
  updateState: UpdateState;
  releaseNotes?: ReleaseNotes | null;
  progress?: number;
  web?: boolean;
};

function AboutTab({ version, updateState, releaseNotes, progress, web }: AboutTabProps) {
  const changelogHref = version ? `${RELEASES_HREF}/tag/${version}` : RELEASES_HREF;
  return (
    <div className="chast__about">
      <div className="chast__abouthead">
        <img
          src={asset("assets/dcl-logo.png")}
          alt="Decentraland Creator Hub"
          className="chast__aboutlogo"
        />
        <div className="chast__aboutinfo">
          <h5 className="chast__abouttitle">{COPY.about.title}</h5>
          <div className="chast__aboutversion">
            {version && <span className="chast__aboutver">{`v${version}`}</span>}
            <a
              className="chast__changelog"
              href={changelogHref}
              target="_blank"
              rel="noreferrer noopener"
            >
              {COPY.about.viewChangelog}
            </a>
          </div>
        </div>
      </div>

      {!web && (
        <div className="chast__about-update-section">
          <UpdateSettings
            updateState={updateState}
            version={version}
            releaseNotes={releaseNotes}
            progress={progress}
          />
        </div>
      )}
    </div>
  );
}

function WebDesktopNotice() {
  return (
    <EmptyState
      className="chast__webnotice"
      title={COPY.web.noticeTitle}
      subtitle={COPY.web.noticeBody}
      actions={
        <Button
          variant="primary"
          onClick={() => {
            if (typeof window !== "undefined") {
              window.location.href = DOWNLOAD_HREF;
            }
          }}
        >
          {COPY.web.download}
        </Button>
      }
    />
  );
}

type ChAppSettingsTabbedSectionsProps = {
  variant?: "modal" | "panel";
  open?: boolean;
  web?: boolean;
  initialTab?: string;
  version?: string;
  settings?: AppSettings;
  editors?: CodeEditor[];
  isCustomScenesPath?: boolean;
  initialError?: string | null;
  updateState?: UpdateState;
  releaseNotes?: ReleaseNotes | null;
  progress?: number;
  onClose?: () => void;
};

export default function ChAppSettingsTabbedSections({
  variant = "modal",
  open = true,
  web = false,
  initialTab = web ? "about" : "scenes",
  version = "",
  settings = {
    scenesPath: "~/Documents/Decentraland",
    dependencyUpdateStrategy: "notify",
    previewOptions: {
      debugger: false,
      enableLandscapeTerrains: true,
      showWarnings: true,
    },
  },
  editors = [
    { name: "Visual Studio Code", path: "/usr/bin/code", isDefault: true },
    { name: "WebStorm", path: "/usr/bin/webstorm" },
  ],
  isCustomScenesPath = true,
  initialError = null,
  updateState = "idle",
  releaseNotes = {
    version: "1.7.0",
    content: [
      "## What's New",
      "- Inspector multi-select and group transforms",
      "- Faster scene preview cold-start",
      "## Bug Fixes",
      "- Fixed crash when importing large .glb assets",
      "- Resolved duplicate-entity ids on paste",
    ],
  },
  progress = 0,
  onClose = () => {},
}: ChAppSettingsTabbedSectionsProps) {
  const [activeTab, setActiveTab] = useState(initialTab);
  const [scenes, setScenes] = useState(settings);
  const [error, setError] = useState<string | null>(initialError);

  const defaultEditor = editors.find((e) => e.isDefault)?.path || "";

  const handleScenesPath = useCallback((value: string) => {
    setScenes((prev) => ({ ...prev, scenesPath: value }));
    setError(null);
  }, []);

  const handleReset = useCallback(() => {
    setScenes((prev) => ({
      ...prev,
      scenesPath: "~/Documents/Decentraland",
    }));
    setError(null);
  }, []);

  const handleOpenFolder = useCallback(() => {
    setScenes((prev) => ({ ...prev, scenesPath: "~/Projects" }));
    setError(null);
  }, []);

  const handlePreviewOptionChange = useCallback(
    (key: keyof PreviewOptions, checked: boolean) => {
      setScenes((prev) => ({
        ...prev,
        previewOptions: { ...prev.previewOptions, [key]: checked },
      }));
    },
    [],
  );

  if (!open) return null;

  return (
    <ChDialogShell
      variant={variant}
      className="chast"
      icon={<SettingsIcon />}
      title={COPY.title}
      ariaLabel={COPY.title}
      onClose={onClose}
      tabs={COPY.tabs}
      activeTab={activeTab}
      onTabChange={setActiveTab}
    >
      {activeTab === "scenes" &&
        (web ? (
          <WebDesktopNotice />
        ) : (
          <ScenesTab
            settings={scenes}
            error={error}
            isCustomScenesPath={isCustomScenesPath}
            onChange={handleScenesPath}
            onReset={handleReset}
            onOpenFolder={handleOpenFolder}
          />
        ))}
      {activeTab === "editor" &&
        (web ? (
          <WebDesktopNotice />
        ) : (
          <EditorTab
            settings={scenes}
            editors={editors}
            defaultEditor={defaultEditor}
            onPreviewOptionChange={handlePreviewOptionChange}
          />
        ))}
      {activeTab === "about" && (
        <AboutTab
          version={version}
          updateState={updateState}
          releaseNotes={releaseNotes}
          progress={progress}
          web={web}
        />
      )}
    </ChDialogShell>
  );
}
