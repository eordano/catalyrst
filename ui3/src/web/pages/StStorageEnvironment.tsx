import { useState } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import Spinner from "../../atoms/Spinner";
import "./ststorageenvironment.css";

const C = "ststorageenvironment";

const T = {
  back: "Back",
  position: "Position",
  envTab: "Environment",
  sceneTab: "Scene",
  playerTab: "Player",
  title: "Environment Variables",
  add: "Add",
  clearAll: "Clear All",
  noKeys: "No environment variables found",
  key: "Key",
  actions: "Actions",
};

export type EnvKey = { key: string };

export type Scope = { realm?: string | null; position?: string | null };

type DialogState = "add" | "clear" | { edit: string } | { delete: string } | null;

const ArrowBackGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z" fill="currentColor" />
  </svg>
);
const FmdGoodGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5a2.5 2.5 0 1 1 0-5 2.5 2.5 0 0 1 0 5z"
      fill="currentColor"
    />
  </svg>
);
const SettingsGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M19.14 12.94a7.49 7.49 0 0 0 .05-.94 7.49 7.49 0 0 0-.05-.94l2.03-1.58a.5.5 0 0 0 .12-.61l-1.92-3.32a.5.5 0 0 0-.61-.22l-2.39.96a7 7 0 0 0-1.62-.94l-.36-2.54a.5.5 0 0 0-.5-.42h-3.84a.5.5 0 0 0-.5.42l-.36 2.54c-.59.24-1.13.56-1.62.94l-2.39-.96a.5.5 0 0 0-.61.22L2.74 8.87a.5.5 0 0 0 .12.61l2.03 1.58c-.03.31-.05.62-.05.94 0 .32.02.63.05.94l-2.03 1.58a.5.5 0 0 0-.12.61l1.92 3.32c.13.24.42.32.61.22l2.39-.96c.49.38 1.03.7 1.62.94l.36 2.54c.05.24.25.42.5.42h3.84c.25 0 .45-.18.5-.42l.36-2.54c.59-.24 1.13-.56 1.62-.94l2.39.96c.24.1.49.02.61-.22l1.92-3.32a.5.5 0 0 0-.12-.61l-2.03-1.58zM12 15.5A3.5 3.5 0 1 1 12 8.5a3.5 3.5 0 0 1 0 7z"
      fill="currentColor"
    />
  </svg>
);
const ViewInArGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M3 4a1 1 0 0 1 1-1h2v2H5v1H3V4zm16-1h2a1 1 0 0 1 1 1v2h-2V5h-1V3zM3 18h2v1h1v2H4a1 1 0 0 1-1-1v-2zm18 0h2v2a1 1 0 0 1-1 1h-2v-2h1v-1zM12 6.5l5 2.9v5.2l-5 2.9-5-2.9V9.4l5-2.9zm0 2.31L9 10.6v2.8l3 1.79 3-1.79v-2.8l-3-1.79z"
      fill="currentColor"
    />
  </svg>
);
const PeopleGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z"
      fill="currentColor"
    />
  </svg>
);
const AddGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z" fill="currentColor" />
  </svg>
);
const DeleteSweepGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M15 16h4v2h-4v-2zm0-8h7v2h-7V8zm0 4h6v2h-6v-2zM3 18c0 1.1.9 2 2 2h6c1.1 0 2-.9 2-2V8H3v10zM14 5h-3l-1-1H6L5 5H2v2h12V5z"
      fill="currentColor"
    />
  </svg>
);
const EditGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M3 17.25V21h3.75L17.81 9.94l-3.75-3.75L3 17.25zM20.71 7.04a.996.996 0 0 0 0-1.41l-2.34-2.34a.996.996 0 0 0-1.41 0l-1.83 1.83 3.75 3.75 1.83-1.83z"
      fill="currentColor"
    />
  </svg>
);
const DeleteGlyph = () => (
  <svg viewBox="0 0 24 24" aria-hidden="true">
    <path
      d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"
      fill="currentColor"
    />
  </svg>
);

const STORAGE_TABS = [
  { value: "env", label: T.envTab, icon: <SettingsGlyph /> },
  { value: "scene", label: T.sceneTab, icon: <ViewInArGlyph /> },
  { value: "players", label: T.playerTab, icon: <PeopleGlyph /> },
];

type StStorageEnvironmentProps = {
  chrome?: boolean;
  envKeys?: EnvKey[];
  scope?: Scope;
  isLoading?: boolean;
  activeTab?: string;
  embedded?: boolean;
};

export default function StStorageEnvironment({
  chrome = true,
  envKeys = [],
  scope,
  isLoading = false,
  activeTab = "env",
  embedded = false,
}: StStorageEnvironmentProps) {
  const [, setOpenDialog] = useState<DialogState>(null);
  const keys = envKeys ?? [];
  const hasKeys = keys.length > 0;
  const scopeLabel = scope?.realm ?? scope?.position ?? "";

  const body = (
      <div className={C}>
        <div className={`${C}__header`}>
          <button type="button" className={`${C}__back`} aria-label={T.back}>
            <ArrowBackGlyph />
            <span>{T.back}</span>
          </button>
          {scopeLabel ? (
            <div className={`${C}__scoperow`}>
              <span className={`${C}__chip`}>
                <FmdGoodGlyph />
                <span className={`${C}__chiplabel`}>{scopeLabel}</span>
              </span>
              {scope?.realm && scope?.position ? (
                <span className={`${C}__pos`}>
                  {T.position}: {scope.position}
                </span>
              ) : null}
            </div>
          ) : null}
        </div>

        <div className={`${C}__tabsroot`}>
          <div className={`${C}__tabs`} role="tablist" aria-label="storage sections">
            {STORAGE_TABS.map((tab) => (
              <button
                key={tab.value}
                type="button"
                role="tab"
                aria-selected={tab.value === activeTab}
                className={`${C}__tab` + (tab.value === activeTab ? " is-active" : "")}
              >
                {tab.icon}
                {tab.label}
              </button>
            ))}
          </div>
        </div>

        <div className={`${C}__sectionhead`}>
          <h2 className={`${C}__title`}>{T.title}</h2>
          <div className={`${C}__actions`}>
            <button
              type="button"
              className={`${C}__btn ${C}__btn--contained`}
              onClick={() => setOpenDialog("add")}
            >
              <AddGlyph />
              {T.add}
            </button>
            {hasKeys ? (
              <button
                type="button"
                className={`${C}__btn ${C}__btn--outerror`}
                onClick={() => setOpenDialog("clear")}
              >
                <DeleteSweepGlyph />
                {T.clearAll}
              </button>
            ) : null}
          </div>
        </div>

        {isLoading ? (
          <div className={`${C}__loading`}>
            <Spinner size={40} />
          </div>
        ) : hasKeys ? (
          <div className={`${C}__tablewrap`}>
            <table className={`${C}__table`}>
              <thead>
                <tr>
                  <th>{T.key}</th>
                  <th className="is-right">{T.actions}</th>
                </tr>
              </thead>
              <tbody>
                {keys.map((item) => (
                  <tr key={item.key}>
                    <td className={`${C}__keycell`}>{item.key}</td>
                    <td className="is-right">
                      <button
                        type="button"
                        className={`${C}__iconbtn ${C}__iconbtn--edit`}
                        aria-label={`edit ${item.key}`}
                        onClick={() => setOpenDialog({ edit: item.key })}
                      >
                        <EditGlyph />
                      </button>
                      <button
                        type="button"
                        className={`${C}__iconbtn ${C}__iconbtn--delete`}
                        aria-label={`delete ${item.key}`}
                        onClick={() => setOpenDialog({ delete: item.key })}
                      >
                        <DeleteGlyph />
                      </button>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        ) : (
          <div className={`${C}__empty`}>{T.noKeys}</div>
        )}
      </div>
  );

  return <SitesChromeMaybe chrome={chrome && !embedded} active="play">{body}</SitesChromeMaybe>;
}
