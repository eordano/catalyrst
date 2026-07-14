import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import Button from "../../atoms/Button";
import ChDialogShell from "./ChDialogShell";
import "./chworldpermissionstabbedsections.css";
import { hueFor } from "../../data/format";

interface Wallet {
  address: string;
  name?: string;
  role?: string;
}

interface Collaborator {
  address: string;
  name?: string;
  deployment: string;
  parcelsCount: number;
}

const COPY = {
  title: (worldName: string) => `Permissions - ${worldName}`,
  tabs: { access: "Access", collaborators: "Collaborators" },
  access: {
    title: "Manage who can access your World",
    type: {
      public: "Public",
      invitation_only: "Invitation only",
      password_protected: "Password protected",
      public_description: "Anyone can access this world",
      invitation_only_description:
        "Only addresses and communities included in the whitelist can join.",
      password_protected_description:
        "Only users who know the access password can join",
    },
    approved_addresses: (n: number) => `Approved Addresses: ${n}`,
    new_invite: "New Invite",
    clear_list: "Clear List",
    empty_list: "You haven't approved any addresses yet",
    remove: "Remove",
  },
  collaborators: {
    description:
      "Add up to 100 collaborators and manage their permission to deploy, or stream into your World.",
    column_name_label: (n: string | number) => `Collaborators: ${n}`,
    add: "Add",
    clear_list: "Clear List",
    empty_list: "You haven't added any collaborators yet",
    deployment: {
      world_wide: "All Parcels",
      parcels: "Custom Coordinates",
      none: "None",
    },
    parcels_count: (n: number) => `${n} Parcels`,
  },
  parcels: {
    title: "Custom Coordinates",
    description: "Click individual tiles to include/exclude them in the layout.",
    parcels_count: (n: number) =>
      `Selecting ${n} ${n === 1 ? "parcel" : "parcels"}`,
    discard: "Discard",
    confirm: "Confirm",
  },
  roles: { owner: "Owner", collaborator: "Collaborator" },
};

const MAX_COLLABORATORS = 100;

const LockIcon = () => (
  <svg viewBox="0 0 24 24" className="chwp__svg" aria-hidden="true">
    <path
      fill="currentColor"
      d="M18 8h-1V6c0-2.76-2.24-5-5-5S7 3.24 7 6v2H6c-1.1 0-2 .9-2 2v10c0 1.1.9 2 2 2h12c1.1 0 2-.9 2-2V10c0-1.1-.9-2-2-2zM9 6c0-1.66 1.34-3 3-3s3 1.34 3 3v2H9V6zm3 11c-1.1 0-2-.9-2-2s.9-2 2-2 2 .9 2 2-.9 2-2 2z"
    />
  </svg>
);
const PublicIcon = () => (
  <svg viewBox="0 0 24 24" className="chwp__svg" aria-hidden="true">
    <path
      fill="currentColor"
      d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm-1 17.93c-3.95-.49-7-3.85-7-7.93 0-.62.08-1.21.21-1.79L9 15v1c0 1.1.9 2 2 2v1.93zm6.9-2.54c-.26-.81-1-1.39-1.9-1.39h-1v-3c0-.55-.45-1-1-1H8v-2h2c.55 0 1-.45 1-1V7h2c1.1 0 2-.9 2-2v-.41c2.93 1.19 5 4.06 5 7.41 0 2.08-.8 3.97-2.1 5.39z"
    />
  </svg>
);
const AddIcon = () => (
  <svg viewBox="0 0 24 24" className="chwp__svg" aria-hidden="true">
    <path fill="currentColor" d="M19 13h-6v6h-2v-6H5v-2h6V5h2v6h6v2z" />
  </svg>
);
const DeleteIcon = () => (
  <svg viewBox="0 0 24 24" className="chwp__svg" aria-hidden="true">
    <path
      fill="currentColor"
      d="M6 19c0 1.1.9 2 2 2h8c1.1 0 2-.9 2-2V7H6v12zM19 4h-3.5l-1-1h-5l-1 1H5v2h14V4z"
    />
  </svg>
);
const ArrowBackIcon = () => (
  <svg viewBox="0 0 24 24" className="chwp__svg" aria-hidden="true">
    <path
      fill="currentColor"
      d="M20 11H7.83l5.59-5.59L12 4l-8 8 8 8 1.41-1.41L7.83 13H20v-2z"
    />
  </svg>
);
const ChevronIcon = () => (
  <svg viewBox="0 0 24 24" className="chwp__chevron" aria-hidden="true">
    <path fill="currentColor" d="M7 10l5 5 5-5z" />
  </svg>
);

const shorten = (addr: string) =>
  addr.length > 12 ? `${addr.slice(0, 6)}...${addr.slice(-4)}` : addr;


function AvatarInfo({ value, name }: { value: string; name?: string }) {
  return (
    <div className="chwp__avatar">
      <span
        className="chwp__face u-avatar"
        style={
          {
            "--sz": "32px",
            "--hue": hueFor(value),
          } as CSSProperties & { "--sz": string; "--hue": number }
        }
      />
      <span className="chwp__paragraph">
        {name && <span className="chwp__name">{name}</span>}
        <span className="chwp__addr">{shorten(value)}</span>
      </span>
    </div>
  );
}

function PermissionRow({
  value,
  name,
  role,
  control,
  onRemove,
}: {
  value: string;
  name?: string;
  role?: string;
  control?: ReactNode;
  onRemove?: () => void;
}) {
  const roleLabel =
    role === "owner"
      ? COPY.roles.owner
      : role === "collaborator"
        ? COPY.roles.collaborator
        : null;
  return (
    <div className={"chwp__item" + (control ? " chwp__item--collab" : "")}>
      <div className="chwp__iteminfo">
        <AvatarInfo value={value} name={name} />
        {roleLabel && <span className="chwp__badge">{roleLabel}</span>}
      </div>
      <div className="chwp__itemcontrol">{control}</div>
      {onRemove ? (
        <button
          type="button"
          className="chwp__rowmenu"
          aria-label={COPY.access.remove}
          onClick={onRemove}
        >
          <DeleteIcon />
        </button>
      ) : (
        <span className="chwp__rowmenu chwp__rowmenu--empty" />
      )}
    </div>
  );
}

const ACCESS_TYPE_OPTIONS = [
  {
    value: "unrestricted",
    label: COPY.access.type.public,
    icon: <PublicIcon />,
    description: COPY.access.type.public_description,
  },
  {
    value: "allow-list",
    label: COPY.access.type.invitation_only,
    icon: <LockIcon />,
    description: COPY.access.type.invitation_only_description,
  },
  {
    value: "shared-secret",
    label: COPY.access.type.password_protected,
    icon: <LockIcon />,
    description: COPY.access.type.password_protected_description,
  },
];

function AccessTab({
  ownerAddress,
  ownerName,
  wallets,
  accessType: initialAccessType,
  onClearList,
  onNewInvite,
}: {
  ownerAddress: string;
  ownerName?: string;
  wallets: Wallet[];
  accessType: string;
  onClearList?: () => void;
  onNewInvite?: () => void;
}) {
  const [accessType, setAccessType] = useState(initialAccessType);
  const [list, setList] = useState(wallets);
  const current = ACCESS_TYPE_OPTIONS.find((o) => o.value === accessType);
  const isInvitationOnly = accessType === "allow-list";
  const total = list.length;

  return (
    <div className={"chwp__accesstab" + (accessType !== "unrestricted" ? " is-restricted" : "")}>
      <h6 className="chwp__sectiontitle">{COPY.access.title}</h6>

      <div className="chwp__accesstyperow">
        <div className="chwp__select">
          <select
            className="chwp__nativeselect"
            value={accessType}
            onChange={(e) => setAccessType(e.target.value)}
            aria-label="Access type"
          >
            {ACCESS_TYPE_OPTIONS.map((o) => (
              <option key={o.value} value={o.value}>
                {o.label}
              </option>
            ))}
          </select>
          <span className="chwp__selectvalue">
            {current?.icon}
            {current?.label}
          </span>
          <ChevronIcon />
        </div>
        <p className="chwp__accessdesc">{current?.description}</p>
      </div>

      {isInvitationOnly && (
        <div className="chwp__accessform">
          <div className="chwp__listheader">
            <span className="chwp__approvedcount">
              {COPY.access.approved_addresses(total)}
            </span>
            <div className="chwp__listactions">
              {onClearList && (
                <button
                  type="button"
                  className="chwp__textlink"
                  onClick={onClearList}
                >
                  {COPY.access.clear_list}
                </button>
              )}
              {onNewInvite && (
                <Button variant="primary" className="chwp__btn--icon" onClick={onNewInvite}>
                  <AddIcon />
                  {COPY.access.new_invite}
                </Button>
              )}
            </div>
          </div>

          <div className="chwp__list">
            {[ownerAddress, ...list.map((w) => w.address)].length === 0 ? (
              <p className="chwp__emptylist">{COPY.access.empty_list}</p>
            ) : (
              <>
                <PermissionRow
                  value={ownerAddress}
                  name={ownerName}
                  role="owner"
                />
                {list.map((w) => (
                  <PermissionRow
                    key={w.address}
                    value={w.address}
                    name={w.name}
                    role={w.role}
                    onRemove={
                      w.role
                        ? undefined
                        : () =>
                            setList((prev) =>
                              prev.filter((x) => x.address !== w.address),
                            )
                    }
                  />
                ))}
              </>
            )}
          </div>
        </div>
      )}
    </div>
  );
}

function DeploymentSelect({
  value,
  parcelsCount,
  onPickParcels,
}: {
  value: string;
  parcelsCount: number;
  onPickParcels?: () => void;
}) {
  const renderLabel =
    value === "none"
      ? COPY.collaborators.deployment.none
      : value === "parcels"
        ? COPY.collaborators.parcels_count(parcelsCount)
        : COPY.collaborators.deployment.world_wide;
  return (
    <div className="chwp__deployselect">
      <select
        className="chwp__nativeselect"
        value={value}
        onChange={(e) => {
          if (e.target.value === "parcels") onPickParcels?.();
        }}
        aria-label="Deployment scope"
      >
        <option value="world-wide">{COPY.collaborators.deployment.world_wide}</option>
        <option value="parcels">{COPY.collaborators.deployment.parcels}</option>
        {value === "none" && (
          <option value="none">{COPY.collaborators.deployment.none}</option>
        )}
      </select>
      <span className="chwp__deployvalue">{renderLabel}</span>
      <ChevronIcon />
    </div>
  );
}

function CollaboratorsTab({
  ownerAddress,
  ownerName,
  collaborators,
  onPickParcels,
  onClearList,
  onAddCollaborator,
  onRemoveCollaborator,
}: {
  ownerAddress: string;
  ownerName?: string;
  collaborators: Collaborator[];
  onPickParcels: (c: Collaborator) => void;
  onClearList?: () => void;
  onAddCollaborator?: () => void;
  onRemoveCollaborator?: (c: Collaborator) => void;
}) {
  const count = collaborators.length;
  if (count === 0) {
    return (
      <div className="chwp__collabtab">
        <p className="chwp__collabdesc">
          <span>Add up to 100 collaborators</span> and manage their permission to
          deploy, or stream into your World.
        </p>
        <div className="chwp__emptystate">
          <p>{COPY.collaborators.empty_list}</p>
          {onAddCollaborator && (
            <Button variant="primary" className="chwp__btn--icon" onClick={onAddCollaborator}>
              <AddIcon />
              {COPY.collaborators.add}
            </Button>
          )}
        </div>
      </div>
    );
  }
  return (
    <div className="chwp__collabtab">
      <p className="chwp__collabdesc">
        <span>Add up to 100 collaborators</span> and manage their permission to
        deploy, or stream into your World.
      </p>
      <div className="chwp__collablist">
        <div className="chwp__collabheaderrow">
          <span className="chwp__collabheadertitle">
            {COPY.collaborators.column_name_label(`${count}/${MAX_COLLABORATORS}`)}
          </span>
          {onClearList && (
            <button
              type="button"
              className="chwp__textlink"
              onClick={onClearList}
            >
              {COPY.collaborators.clear_list}
            </button>
          )}
          {onAddCollaborator && (
            <Button variant="primary" className="chwp__btn--icon" onClick={onAddCollaborator}>
              <AddIcon />
              {COPY.collaborators.add}
            </Button>
          )}
        </div>
        <PermissionRow
          value={ownerAddress}
          name={ownerName}
          role="owner"
        />
        {collaborators.map((c) => (
          <PermissionRow
            key={c.address}
            value={c.address}
            name={c.name}
            role="collaborator"
            onRemove={
              onRemoveCollaborator ? () => onRemoveCollaborator(c) : undefined
            }
            control={
              <DeploymentSelect
                value={c.deployment}
                parcelsCount={c.parcelsCount}
                onPickParcels={() => onPickParcels(c)}
              />
            }
          />
        ))}
      </div>
    </div>
  );
}

function ParcelsTab({
  collaborator,
  selectedParcels,
  onGoBack,
}: {
  collaborator: Collaborator;
  selectedParcels?: string[];
  onGoBack: () => void;
}) {
  const SIZE = 9;
  const initial = () => new Set<string>(selectedParcels ?? []);
  const [selected, setSelected] = useState(initial);
  const [dirty, setDirty] = useState(false);

  const toggle = (key: string) => {
    setSelected((prev) => {
      const next = new Set(prev);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
    setDirty(true);
  };

  return (
    <div className="chwp__parcelstab">
      <div className="chwp__parcelheader">
        <button
          type="button"
          className="chwp__back"
          aria-label="Go back"
          onClick={onGoBack}
        >
          <ArrowBackIcon />
        </button>
        <h6 className="chwp__sectiontitle">{COPY.parcels.title}</h6>
      </div>
      <p className="chwp__parceldesc">{COPY.parcels.description}</p>

      <div className="chwp__atlas">
        <div className="chwp__atlasfloating">
          <AvatarInfo value={collaborator.address} name={collaborator.name} />
        </div>
        <div
          className="chwp__grid"
          style={{ "--cols": SIZE } as CSSProperties & { "--cols": number }}
        >
          {Array.from({ length: SIZE * SIZE }).map((_, i) => {
            const x = i % SIZE;
            const y = Math.floor(i / SIZE);
            const key = `${x},${y}`;
            const on = selected.has(key);
            return (
              <button
                key={key}
                type="button"
                className={"chwp__tile" + (on ? " is-selected" : "")}
                aria-pressed={on}
                aria-label={`Parcel ${x},${y}`}
                onClick={() => toggle(key)}
              />
            );
          })}
        </div>
      </div>

      <div className="chwp__parcelactions">
        {dirty && (
          <Button
            variant="ghost"
            onClick={() => {
              setSelected(initial());
              setDirty(false);
            }}
          >
            {COPY.parcels.discard}
          </Button>
        )}
        <span className="chwp__parcelcount">
          {COPY.parcels.parcels_count(selected.size)}
        </span>
        <Button
          variant="primary"
          className="chwp__savebtn"
          disabled={!dirty}
          onClick={onGoBack}
        >
          {COPY.parcels.confirm}
        </Button>
      </div>
    </div>
  );
}

const TABS = [
  { value: "access", label: COPY.tabs.access },
  { value: "collaborators", label: COPY.tabs.collaborators },
];

export default function ChWorldPermissionsTabbedSections({
  variant = "modal",
  open = true,
  worldName = "",
  initialTab = "access",
  onClose = () => {},
  accessType = "allow-list",
  ownerAddress = "",
  ownerName = undefined,
  accessWallets = [],
  collaborators = [],
  onClearList = undefined,
  onNewInvite = undefined,
  onAddCollaborator = undefined,
  onRemoveCollaborator = undefined,
}: {
  variant?: "modal" | "panel";
  open?: boolean;
  worldName?: string;
  initialTab?: string;
  onClose?: () => void;
  accessType?: string;
  ownerAddress?: string;
  ownerName?: string;
  accessWallets?: Wallet[];
  collaborators?: Collaborator[];
  onClearList?: () => void;
  onNewInvite?: () => void;
  onAddCollaborator?: () => void;
  onRemoveCollaborator?: (c: Collaborator) => void;
}) {
  const [activeTab, setActiveTab] = useState(
    initialTab === "parcels" ? "collaborators" : initialTab,
  );
  const [parcelsFor, setParcelsFor] = useState<Collaborator | null>(
    initialTab === "parcels" ? (collaborators[1] ?? null) : null,
  );

  if (!open) return null;

  return (
    <ChDialogShell
      variant={variant}
      className="chwp"
      width={900}
      icon={<LockIcon />}
      title={COPY.title(worldName)}
      ariaLabel={COPY.title(worldName)}
      onClose={onClose}
      tabs={TABS}
      activeTab={activeTab}
      onTabChange={(value) => {
        setActiveTab(value);
        setParcelsFor(null);
      }}
    >
      {activeTab === "access" && (
        <AccessTab
          ownerAddress={ownerAddress}
          ownerName={ownerName}
          accessType={accessType}
          wallets={accessWallets}
          onClearList={onClearList}
          onNewInvite={onNewInvite}
        />
      )}
      {activeTab === "collaborators" &&
        (parcelsFor ? (
          <ParcelsTab
            collaborator={parcelsFor}
            onGoBack={() => setParcelsFor(null)}
          />
        ) : (
          <CollaboratorsTab
            ownerAddress={ownerAddress}
            ownerName={ownerName}
            collaborators={collaborators}
            onPickParcels={(c) => setParcelsFor(c)}
            onClearList={onClearList}
            onAddCollaborator={onAddCollaborator}
            onRemoveCollaborator={onRemoveCollaborator}
          />
        ))}
    </ChDialogShell>
  );
}
