import { useId, useMemo, useState } from "react";
import type { CSSProperties, ReactNode } from "react";
import { ChevronLeft, ChevronDownAlt } from "../../atoms/icons";
import Button from "../../atoms/Button";
import ChDialogShell from "./ChDialogShell";
import "./chmodalworldpermissions.css";

type VarStyle = CSSProperties & { [k: `--${string}`]: string | number };
const cssVars = (s: VarStyle): CSSProperties => s;

const LockIcon = ({ size = 20 }: { size?: number }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <rect x="5" y="10.5" width="14" height="10" rx="2" fill="currentColor" />
    <path d="M8 10.5V8a4 4 0 0 1 8 0v2.5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" />
  </svg>
);
const PublicIcon = ({ size = 16 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.7" fill="none" />
    <path d="M3 12h18M12 3c2.6 2.4 4 5.6 4 9s-1.4 6.6-4 9c-2.6-2.4-4-5.6-4-9s1.4-6.6 4-9Z" stroke="currentColor" strokeWidth="1.7" fill="none" />
  </svg>
);
const AddIcon = ({ size = 18 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="2.2" strokeLinecap="round" />
  </svg>
);
const MoreIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <circle cx="12" cy="5" r="1.7" fill="currentColor" />
    <circle cx="12" cy="12" r="1.7" fill="currentColor" />
    <circle cx="12" cy="19" r="1.7" fill="currentColor" />
  </svg>
);
const InfoIcon = ({ size = 14 }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
    <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" fill="none" />
    <path d="M12 11v5M12 7.6v.2" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
  </svg>
);
const SearchIcon = () => (
  <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true">
    <circle cx="11" cy="11" r="7" stroke="currentColor" strokeWidth="1.8" fill="none" />
    <path d="M16.5 16.5L21 21" stroke="currentColor" strokeWidth="1.8" strokeLinecap="round" />
  </svg>
);
const EyeIcon = () => (
  <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
    <path d="M2 12s4-7 10-7 10 7 10 7-4 7-10 7-10-7-10-7Z" stroke="currentColor" strokeWidth="1.6" fill="none" />
    <circle cx="12" cy="12" r="3" stroke="currentColor" strokeWidth="1.6" fill="none" />
  </svg>
);

const WORLD_NAME = "mystore.dcl.eth";
const OWNER = "0x9f3c2b1a7d8e4c5f6a0b1c2d3e4f5a6b7c8d9e21";

type AccessType = "unrestricted" | "allowList" | "sharedSecret";
type AccessOption = {
  value: AccessType;
  label: string;
  icon: ReactNode;
  description: string;
};
type Community = { id: string; name: string; membersCount: number };
type Collaborator = {
  address: string;
  role: "owner" | "collaborator";
  deployment: "world_wide" | "parcels" | "none";
  streaming: boolean;
  parcels: number;
};

const ACCESS_WALLETS = [
  OWNER,
  "0x4a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d",
  "0x12ab34cd56ef7890ab12cd34ef5678901234abcd",
  "0x778899aabbccddeeff00112233445566778899aa",
];
const ACCESS_COMMUNITIES: Community[] = [
  { id: "aa11bb22-cc33-dd44-ee55-ff6677889900", name: "Wearable Wizards", membersCount: 248 },
  { id: "bb22cc33-dd44-ee55-ff66-7788990011aa", name: "DCL Builders Guild", membersCount: 1320 },
];

const COLLABORATORS: [Collaborator, Collaborator, Collaborator, Collaborator] = [
  { address: OWNER, role: "owner", deployment: "world_wide", streaming: true, parcels: 0 },
  { address: "0x4a1b2c3d4e5f60718293a4b5c6d7e8f90a1b2c3d", role: "collaborator", deployment: "world_wide", streaming: true, parcels: 0 },
  { address: "0x12ab34cd56ef7890ab12cd34ef5678901234abcd", role: "collaborator", deployment: "parcels", streaming: false, parcels: 6 },
  { address: "0x778899aabbccddeeff00112233445566778899aa", role: "collaborator", deployment: "none", streaming: true, parcels: 0 },
];

function shorten(addr: string) {
  return addr.slice(0, 6) + "\u{2026}" + addr.slice(-4);
}
function hueOf(addr: string) {
  let h = 0;
  for (let i = 2; i < addr.length; i++) h = (h * 31 + addr.charCodeAt(i)) % 360;
  return h;
}

const ACCESS_TYPE_OPTIONS: [AccessOption, AccessOption, AccessOption] = [
  { value: "unrestricted", label: "Public", icon: <PublicIcon />, description: "Anyone can access this World" },
  { value: "allowList", label: "Invitation only", icon: <LockIcon size={16} />, description: "Only addresses and communities included in the whitelist can join." },
  { value: "sharedSecret", label: "Password protected", icon: <LockIcon size={16} />, description: "Only users who know the access password can join" },
];

function Avatar({ addr, size = 32 }: { addr: string; size?: number }) {
  return <span className="wp__avatar" style={cssVars({ "--sz": size + "px", "--hue": hueOf(addr) })} />;
}

function PermSelect({ value, className = "" }: { value: ReactNode; className?: string }) {
  return (
    <button type="button" className={"wp__permselect " + className}>
      <span className="wp__permselectval">{value}</span>
      <ChevronDownAlt size={18} />
    </button>
  );
}

function PrimaryBtn({
  children,
  icon,
  disabled,
}: {
  children: ReactNode;
  icon?: ReactNode;
  disabled?: boolean;
}) {
  return (
    <Button variant="primary" disabled={disabled}>
      {icon}
      {children}
    </Button>
  );
}
function SecondaryBtn({ children, onClick }: { children: ReactNode; onClick?: () => void }) {
  return <Button variant="secondary" onClick={onClick}>{children}</Button>;
}

function AvatarWithInfo({
  addr,
  name,
  subtitle,
  icon,
}: {
  addr: string;
  name?: string;
  subtitle?: string;
  icon?: ReactNode;
}) {
  if (icon) {
    return (
      <div className="wp__avinfo">
        {icon}
        <span className="wp__paragraph">
          {name && <span className="wp__name">{name}</span>}
          {subtitle && <span>{subtitle}</span>}
        </span>
      </div>
    );
  }
  return (
    <div className="wp__avinfo">
      <Avatar addr={addr} />
      <span className="wp__paragraph">
        {name && <span className="wp__name">{name}</span>}
        <span>{shorten(addr)}</span>
      </span>
    </div>
  );
}

function AccessItem({
  addr,
  role,
  name,
  subtitle,
  icon,
}: {
  addr: string;
  role?: "owner" | "collaborator";
  name?: string;
  subtitle?: string;
  icon?: ReactNode;
}) {
  return (
    <div className="wp__item">
      <div className="wp__iteminfo">
        <AvatarWithInfo addr={addr} name={name} subtitle={subtitle} icon={icon} />
        {role ? <span className="wp__rolebadge">{role === "owner" ? "Owner" : "Collaborator"}</span> : null}
      </div>
      <div />
      {role ? <div /> : (
        <button type="button" className="wp__moreicon" aria-label="actions"><MoreIcon /></button>
      )}
    </div>
  );
}

function CollaboratorItem({ c }: { c: Collaborator }) {
  const deployLabel =
    c.deployment === "world_wide" ? "All Parcels"
      : c.deployment === "parcels" ? c.parcels + " Parcels"
        : "None";
  return (
    <div className="wp__item wp__item--collab">
      <div className="wp__iteminfo">
        <AvatarWithInfo addr={c.address} />
        <span className="wp__rolebadge">{c.role === "owner" ? "Owner" : "Collaborator"}</span>
      </div>
      {c.role !== "owner" ? (
        <PermSelect value={deployLabel} className="wp__deployselect" />
      ) : <div />}
      {c.role !== "owner" ? (
        <button type="button" className="wp__moreicon" aria-label="actions"><MoreIcon /></button>
      ) : <div />}
    </div>
  );
}

function AccessDefault({ accessType }: { accessType: AccessType }) {
  const isPublic = accessType === "unrestricted";
  const isAllowList = accessType === "allowList";
  const isPassword = accessType === "sharedSecret";
  const opt = ACCESS_TYPE_OPTIONS.find((o) => o.value === accessType) || ACCESS_TYPE_OPTIONS[1];

  const nonOwner = ACCESS_WALLETS.filter((w) => w !== OWNER);
  const totalInvited = ACCESS_WALLETS.length + ACCESS_COMMUNITIES.reduce((s, c) => s + c.membersCount, 0);

  return (
    <div className={"wp__accesstab" + (isPublic ? "" : " wp__accesstab--restricted")}>
      <h6 className="wp__sectiontitle">Manage who can access your World</h6>

      <div className="wp__accesstyperow">
        <PermSelect
          className="wp__accesstypeselect"
          value={<span className="wp__accesstypeval">{opt.icon}{opt.label}</span>}
        />
        <p className="wp__accesstypedesc">{opt.description}</p>
      </div>

      {isPassword && (
        <div className="wp__passwordsection">
          <PrimaryBtn>Change Password</PrimaryBtn>
        </div>
      )}

      {isAllowList && (
        <div className="wp__accessform">
          <div className="wp__accesslistheader">
            <h6 className="wp__approvedcount">
              Approved Addresses: {totalInvited}
              {totalInvited > 100 && <span className="wp__infoicon"><InfoIcon /></span>}
            </h6>
            <div className="wp__accesslistactions">
              <button type="button" className="wp__clearlist">Clear List</button>
              <PrimaryBtn icon={<AddIcon />}>New Invite</PrimaryBtn>
            </div>
          </div>

          <div className="wp__accesslist">
            <AccessItem addr={OWNER} role="owner" />
            <AccessItem addr={COLLABORATORS[1].address} role="collaborator" />
            {nonOwner
              .filter((w) => w !== COLLABORATORS[1].address)
              .map((w) => <AccessItem key={w} addr={w} />)}
            {ACCESS_COMMUNITIES.map((cm) => (
              <AccessItem
                key={cm.id}
                addr={cm.id}
                name={cm.name}
                subtitle={cm.membersCount + " Members"}
                icon={<span className="wp__communitythumb" style={cssVars({ "--hue": hueOf(cm.id) })} />}
              />
            ))}
          </div>
        </div>
      )}
    </div>
  );
}

function AddUserForm({ inviteTab = "wallet", onClose = () => {} }: { inviteTab?: string; onClose?: () => void }) {
  const tabs = [
    { id: "wallet", label: "Wallet Address" },
    { id: "community", label: "Community" },
    { id: "csv", label: "Import CSV" },
  ];
  return (
    <div className="wp__centered">
      <div className="wp__adduserform">
        <h5 className="wp__formtitle">New Invite</h5>
        <div className="wp__formtabs">
          {tabs.map((tb) => (
            <button
              key={tb.id}
              type="button"
              className={"wp__formtab" + (tb.id === inviteTab ? " is-active" : "")}
            >
              {tb.label}
            </button>
          ))}
        </div>

        {inviteTab === "wallet" && (
          <div className="wp__textfield">
            <input placeholder="0x..." />
          </div>
        )}

        {inviteTab === "community" && (
          <div className="wp__communitysearch">
            <div className="wp__textfield wp__textfield--icon">
              <span className="wp__inputicon"><SearchIcon /></span>
              <input placeholder="Search for a community..." />
            </div>
            <div className="wp__communitydropdown">
              {ACCESS_COMMUNITIES.map((cm) => (
                <div key={cm.id} className="wp__communitydropitem">
                  {cm.name}{" "}
                  <span className="wp__communitydropmembers">({cm.membersCount} Members)</span>
                </div>
              ))}
            </div>
          </div>
        )}

        {inviteTab === "csv" && (
          <div className="wp__csv">
            <div className="wp__csvdrop">
              <p className="wp__csvdroptext">Drop your CSV file here or</p>
              <p className="wp__csvbrowse">Browse your device files</p>
            </div>
          </div>
        )}

        <div className="wp__formactions">
          <SecondaryBtn onClick={onClose}>Cancel</SecondaryBtn>
          <PrimaryBtn>Confirm</PrimaryBtn>
        </div>
      </div>
    </div>
  );
}

function PasswordForm({ isChanging, onClose = () => {} }: { isChanging: boolean; onClose?: () => void }) {
  const uid = useId();
  const pwId = `${uid}-pw`;
  const repeatId = `${uid}-repeat`;
  return (
    <div className="wp__centered">
      <div className="wp__passwordform">
        <h5 className="wp__formtitle">{isChanging ? "Change Password" : "Create New Password"}</h5>

        <div className="wp__pwfield">
          <label className="wp__pwlabel" htmlFor={pwId}>Type your password</label>
          <div className="wp__textfield wp__textfield--icon-end">
            <input id={pwId} type="password" defaultValue={"\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"} />
            <span className="wp__inputicon wp__inputicon--end"><EyeIcon /></span>
          </div>
        </div>

        <div className="wp__pwfield">
          <label className="wp__pwlabel" htmlFor={repeatId}>Repeat your password</label>
          <div className="wp__textfield wp__textfield--icon-end">
            <input id={repeatId} type="password" defaultValue={"\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}"} />
            <span className="wp__inputicon wp__inputicon--end"><EyeIcon /></span>
          </div>
        </div>

        <div className="wp__pwinfo">
          <InfoIcon size={18} />
          <span>Make sure to write down your password so you don't lose it!</span>
        </div>

        <div className="wp__formactions">
          <SecondaryBtn onClick={onClose}>Cancel</SecondaryBtn>
          <PrimaryBtn>Confirm</PrimaryBtn>
        </div>
      </div>
    </div>
  );
}

function ConfirmationPanel({
  title,
  warning,
  cancelLabel,
  confirmLabel,
  onClose = () => {},
}: {
  title: string;
  warning: string;
  cancelLabel: string;
  confirmLabel: string;
  onClose?: () => void;
}) {
  return (
    <div className="wp__centered">
      <div className="wp__confirmpanel">
        <h5 className="wp__confirmtitle">{title}</h5>
        <p className="wp__confirmwarning">{warning}</p>
        <div className="wp__formactions">
          <SecondaryBtn onClick={onClose}>{cancelLabel}</SecondaryBtn>
          <PrimaryBtn>{confirmLabel}</PrimaryBtn>
        </div>
      </div>
    </div>
  );
}

function CollaboratorsDefault() {
  const count = COLLABORATORS.length;
  return (
    <div className="wp__collabtab">
      <h6 className="wp__collabdesc">
        <span>Add up to 100 collaborators</span> and manage their permission to deploy, or stream into your World.
      </h6>

      <div className="wp__collablist">
        <div className="wp__collabheader">
          <h6>Collaborators: {count}/100</h6>
          <button type="button" className="wp__clearlist">Clear List</button>
          <PrimaryBtn icon={<AddIcon />}>Add</PrimaryBtn>
        </div>
        {COLLABORATORS.map((c) => <CollaboratorItem key={c.address} c={c} />)}
      </div>
    </div>
  );
}

function CollaboratorsEmpty() {
  return (
    <div className="wp__collabtab">
      <h6 className="wp__collabdesc">
        <span>Add up to 100 collaborators</span> and manage their permission to deploy, or stream into your World.
      </h6>
      <div className="wp__emptystate">
        <p>You haven't added any collaborators yet</p>
        <PrimaryBtn icon={<AddIcon />}>Add</PrimaryBtn>
      </div>
    </div>
  );
}

function AddCollaboratorDialog({ onClose = () => {} }: { onClose?: () => void }) {
  return (
    <div className="wp__centered">
      <div className="wp__addcollab">
        <h5 className="wp__formtitle">Add Collaborator</h5>
        <div className="wp__textfield">
          <input placeholder="0x..." />
        </div>
        <div className="wp__formactions">
          <SecondaryBtn onClick={onClose}>Cancel</SecondaryBtn>
          <PrimaryBtn>Confirm</PrimaryBtn>
        </div>
      </div>
    </div>
  );
}

function AtlasStub({ collaborator }: { collaborator?: string }) {
  const cells = [];
  for (let r = 0; r < 10; r++) {
    for (let c = 0; c < 22; c++) {
      cells.push(<div key={r + "-" + c} className="wp__parcel" />);
    }
  }
  return (
    <div className="wp__atlas">
      <div className="wp__atlasgrid">{cells}</div>
      {collaborator && (
        <div className="wp__atlasfloat">
          <AvatarWithInfo addr={collaborator} />
        </div>
      )}
    </div>
  );
}

function ParcelsTab({ onClose = () => {} }: { onClose?: () => void }) {
  return (
    <div className="wp__parcelstab">
      <div className="wp__parcelsheader">
        <button type="button" className="wp__parcelsback" aria-label="back"><ChevronLeft size={24} /></button>
        <h6>Custom Coordinates</h6>
      </div>
      <p className="wp__parcelsdesc">Click individual tiles to include/exclude them in the layout.</p>
      <AtlasStub />
      <div className="wp__parcelsactions">
        <SecondaryBtn onClick={onClose}>Discard</SecondaryBtn>
        <span className="wp__parcelscount">Selecting 0 parcels</span>
        <PrimaryBtn>Confirm</PrimaryBtn>
      </div>
    </div>
  );
}

function Loader() {
  return (
    <div className="wp__loader" role="status" aria-label="loading">
      <span className="wp__spinner" aria-hidden="true" />
    </div>
  );
}

type ChModalWorldPermissionsProps = {
  variant?: "modal" | "panel";
  tab?: "access" | "collaborators";
  view?: string;
  accessType?: AccessType;
  inviteTab?: string;
  loading?: boolean;
  onClose?: () => void;
};

export default function ChModalWorldPermissions({
  variant = "modal",
  tab = "access",
  view = "default",
  accessType = "allowList",
  inviteTab = "wallet",
  loading = false,
  onClose = () => {},
}: ChModalWorldPermissionsProps) {
  const [activeTab, setActiveTab] = useState<string>(tab);
  const isParcels = tab === "collaborators" && view === "parcels";

  const TABS: { value: "access" | "collaborators"; label: string }[] = [
    { value: "access", label: "Access" },
    { value: "collaborators", label: "Collaborators" },
  ];

  const body = useMemo(() => {
    if (loading) return <Loader />;

    if (tab === "access") {
      switch (view) {
        case "invite_form":
          return <AddUserForm inviteTab={inviteTab} onClose={onClose} />;
        case "password_form":
          return <PasswordForm isChanging={accessType === "sharedSecret"} onClose={onClose} />;
        case "clear_list_confirm":
          return (
            <ConfirmationPanel
              title="Clear List?"
              warning="If you continue, your list of approved addresses will be erased."
              cancelLabel="Cancel"
              confirmLabel="Confirm"
              onClose={onClose}
            />
          );
        case "change_access_type_confirm":
          return (
            <ConfirmationPanel
              title="Change access type?"
              warning="If you switch world access type, your list of approved addresses will be erased."
              cancelLabel="Cancel"
              confirmLabel="Continue"
              onClose={onClose}
            />
          );
        default:
          return <AccessDefault accessType={accessType} />;
      }
    }

    switch (view) {
      case "add":
        return <AddCollaboratorDialog onClose={onClose} />;
      case "clear_confirmation":
        return (
          <ConfirmationPanel
            title="Clear List?"
            warning="If you continue, your list of collaborators will be erased."
            cancelLabel="Cancel"
            confirmLabel="Confirm"
            onClose={onClose}
          />
        );
      case "empty":
        return <CollaboratorsEmpty />;
      case "parcels":
        return <ParcelsTab onClose={onClose} />;
      default:
        return <CollaboratorsDefault />;
    }
  }, [tab, view, accessType, inviteTab, loading, onClose]);

  return (
    <ChDialogShell
      variant={variant}
      width={900}
      className="wp"
      icon={<LockIcon />}
      title={`Permissions - ${WORLD_NAME}`}
      ariaLabel="World permissions"
      onClose={onClose}
      tabs={TABS}
      activeTab={activeTab}
      onTabChange={setActiveTab}
      hideTabs={isParcels}
    >
      {body}
    </ChDialogShell>
  );
}
