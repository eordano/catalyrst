import type { ReactNode } from "react";
import { useId, useMemo, useState } from "react";
import SearchFieldAtom from "../../atoms/SearchField";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import Modal from "../../components/Modal";
import "./ststorageplayers.css";

const truncateAddress = (address: string): string => {
  if (!address || address.length <= 10) return address;
  return `${address.slice(0, 6)}...${address.slice(-4)}`;
};

const EMPTY_PROFILE_NAMES = new Map<string, string>();

type IconProps = { className?: string };

const ArrowBackIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20z" />
  </svg>
);
const FmdGoodIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="16" height="16" fill="currentColor" aria-hidden="true">
    <path d="M12 2C8.13 2 5 5.13 5 9c0 5.25 7 13 7 13s7-7.75 7-13c0-3.87-3.13-7-7-7zm0 9.5a2.5 2.5 0 1 1 0-5 2.5 2.5 0 0 1 0 5z" />
  </svg>
);
const SettingsIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M19.14 12.94a7.49 7.49 0 0 0 .05-.94 7.49 7.49 0 0 0-.05-.94l2.03-1.58a.5.5 0 0 0 .12-.61l-1.92-3.32a.5.5 0 0 0-.59-.22l-2.39.96a7.03 7.03 0 0 0-1.62-.94l-.36-2.54a.49.49 0 0 0-.5-.42h-3.84a.49.49 0 0 0-.5.42l-.36 2.54a7.03 7.03 0 0 0-1.62.94l-2.39-.96a.5.5 0 0 0-.59.22L2.74 8.87a.5.5 0 0 0 .12.61l2.03 1.58c-.03.31-.05.62-.05.94s.02.63.05.94l-2.03 1.58a.5.5 0 0 0-.12.61l1.92 3.32c.13.22.39.31.59.22l2.39-.96c.5.38 1.05.7 1.62.94l.36 2.54c.04.24.25.42.5.42h3.84c.25 0 .46-.18.5-.42l.36-2.54c.57-.24 1.12-.56 1.62-.94l2.39.96c.2.09.46 0 .59-.22l1.92-3.32a.5.5 0 0 0-.12-.61l-2.03-1.58zM12 15.6A3.6 3.6 0 1 1 12 8.4a3.6 3.6 0 0 1 0 7.2z" />
  </svg>
);
const ViewInArIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M3 6l9-4 9 4v12l-9 4-9-4V6zm9-1.74L6.18 6.7 12 9.28l5.82-2.58L12 4.26zM5 8.18v8.52l6 2.67v-8.4L5 8.18zm14 0l-6 2.79v8.4l6-2.67V8.18z" />
  </svg>
);
const PeopleIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5s-3 1.34-3 3 1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z" />
  </svg>
);
const ClearIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M19 6.41 17.59 5 12 10.59 6.41 5 5 6.41 10.59 12 5 17.59 6.41 19 12 13.41 17.59 19 19 17.59 13.41 12z" />
  </svg>
);
const DeleteSweepIcon = ({ className }: IconProps) => (
  <svg className={className} viewBox="0 0 24 24" width="18" height="18" fill="currentColor" aria-hidden="true">
    <path d="M15 16h4v2h-4v-2zm0-8h7v2h-7V8zm0 4h6v2h-6v-2zM3 18c0 1.1.9 2 2 2h6c1.1 0 2-.9 2-2V8H3v10zM14 5h-3l-1-1H6L5 5H2v2h12V5z" />
  </svg>
);

const STORAGE_TABS = [
  { value: "env", label: "Environment", Icon: SettingsIcon },
  { value: "scene", label: "Scene", Icon: ViewInArIcon },
  { value: "players", label: "Player", Icon: PeopleIcon }
];

function StorageLayout({
  realm,
  position,
  active = "players",
  children,
}: {
  realm?: string | null;
  position?: string | null;
  active?: string;
  children?: ReactNode;
}) {
  const scopeLabel = realm ?? position ?? "";
  return (
    <div className="ststp__container">
      <div className="ststp__header">
        <button type="button" className="ststp__back" aria-label="Back">
          <ArrowBackIcon className="ststp__backicon" />
          <span className="ststp__backlabel">Back</span>
        </button>
        {scopeLabel ? (
          <div className="ststp__scoperow">
            <span className="ststp__scopechip">
              <FmdGoodIcon className="ststp__scopeicon" />
              <span className="ststp__scopelabel">{scopeLabel}</span>
            </span>
            {realm && position ? <span className="ststp__scopepos">Position: {position}</span> : null}
          </div>
        ) : null}
      </div>

      <div className="ststp__tabsroot">
        <div className="ststp__tabs" role="tablist" aria-label="storage sections">
          {STORAGE_TABS.map((tab) => {
            const isActive = tab.value === active;
            const Icon = tab.Icon;
            return (
              <button
                key={tab.value}
                type="button"
                role="tab"
                aria-selected={isActive}
                className={"ststp__tab" + (isActive ? " is-active" : "")}
              >
                <Icon className="ststp__tabicon" />
                <span>{tab.label}</span>
              </button>
            );
          })}
        </div>
      </div>

      {children}
    </div>
  );
}

function SearchField({
  value,
  onChange,
  onClear,
  placeholder,
}: {
  value: string;
  onChange: (value: string) => void;
  onClear: () => void;
  placeholder?: string;
}) {
  return (
    <div className="ststp__search">
      <div className="ststp__searchbox">
        <SearchFieldAtom value={value} onChange={onChange} placeholder={placeholder} />
        {value ? (
          <button type="button" className="ststp__searchclear" onClick={onClear} aria-label="Clear search">
            <ClearIcon />
          </button>
        ) : null}
      </div>
    </div>
  );
}

function PlayerCard({
  address,
  displayName,
  onClick,
}: {
  address: string;
  displayName?: string;
  onClick?: () => void;
}) {
  const label = displayName ?? truncateAddress(address);
  return (
    <button
      type="button"
      className="ststp__card"
      onClick={onClick}
      aria-label={`View storage for ${address}`}
    >
      <span className="ststp__cardtitle">{label}</span>
      <span className="ststp__cardcaption">{truncateAddress(address)}</span>
    </button>
  );
}

function ConfirmDialog({
  open,
  title,
  message,
  confirmLabel,
  cancelLabel,
  onConfirm,
  onCancel,
}: {
  open?: boolean;
  title: string;
  message: string;
  confirmLabel: string;
  cancelLabel: string;
  onConfirm?: () => void;
  onCancel?: () => void;
}) {
  const titleId = useId();
  const descId = useId();
  if (!open) return null;
  return (
    <Modal onClose={onCancel} width="100%" className="modal__card--plain ststp__dialog" ariaLabelledBy={titleId}>
        <h2 className="ststp__dialogtitle" id={titleId}>{title}</h2>
        <div className="ststp__dialogbody">
          <p className="ststp__dialogtext" id={descId}>{message}</p>
        </div>
        <div className="ststp__dialogactions">
          <button type="button" className="ststp__dlgbtn ststp__dlgbtn--text" onClick={onCancel}>
            {cancelLabel}
          </button>
          <button type="button" className="ststp__dlgbtn ststp__dlgbtn--error" onClick={onConfirm}>
            {confirmLabel}
          </button>
        </div>
    </Modal>
  );
}

type StStoragePlayersProps = {
  chrome?: boolean;
  realm?: string | null;
  position?: string | null;
  players?: string[];
  profileNames?: Map<string, string>;
  isLoading?: boolean;
  embedded?: boolean;
};

export default function StStoragePlayers({
  chrome = true,
  realm = null,
  position = null,
  players = [],
  profileNames = EMPTY_PROFILE_NAMES,
  isLoading = false,
  embedded = false
}: StStoragePlayersProps) {
  const [query, setQuery] = useState("");
  const [clearOpen, setClearOpen] = useState(false);

  const playerAddresses = useMemo(() => players ?? [], [players]);

  const filteredPlayers = useMemo(() => {
    if (!query.trim()) return playerAddresses;
    const needle = query.trim().toLowerCase();
    return playerAddresses.filter((address) => {
      if (address.toLowerCase().includes(needle)) return true;
      const name = profileNames.get(address.toLowerCase());
      return name ? name.toLowerCase().includes(needle) : false;
    });
  }, [playerAddresses, query, profileNames]);

  const hasPlayers = playerAddresses.length > 0;

  const body = (
      <div className="ststp">
        <StorageLayout realm={realm} position={position} active="players">
          <div className="ststp__sectionhead">
            <h2 className="ststp__title">Player Storage</h2>
            {hasPlayers ? (
              <button type="button" className="ststp__clearbtn" onClick={() => setClearOpen(true)}>
                <DeleteSweepIcon className="ststp__clearicon" />
                <span>Clear All Players</span>
              </button>
            ) : null}
          </div>

          <p className="ststp__desc">Browse players with stored data, or search by name or address.</p>

          <SearchField
            value={query}
            onChange={setQuery}
            onClear={() => setQuery("")}
            placeholder={"Search by name or address\u{2026}"}
          />

          {isLoading ? (
            <div className="ststp__loading">
              <span className="ststp__spinner" role="status" aria-label="Loading players" />
            </div>
          ) : filteredPlayers.length === 0 ? (
            <p className="ststp__empty">
              {query ? `No results for "${query}"` : "No players found"}
            </p>
          ) : (
            <div className="ststp__grid">
              {filteredPlayers.map((address) => (
                <PlayerCard
                  key={address}
                  address={address}
                  displayName={profileNames.get(address.toLowerCase())}
                  onClick={() => {}}
                />
              ))}
            </div>
          )}

          <ConfirmDialog
            open={clearOpen}
            title="Clear All Player Storage"
            message="Are you sure you want to delete ALL player storage data? This action cannot be undone."
            confirmLabel="Confirm"
            cancelLabel="Cancel"
            onConfirm={() => setClearOpen(false)}
            onCancel={() => setClearOpen(false)}
          />
        </StorageLayout>
      </div>
  );

  return <SitesChromeMaybe chrome={chrome && !embedded} active="create">{body}</SitesChromeMaybe>;
}

export { truncateAddress };
