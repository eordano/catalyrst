import type { CSSProperties, ReactNode } from "react";
import { useState } from "react";
import GovernanceChrome, { type GovernanceNavId } from "../frames/GovernanceChrome";
import DclLogomark from "../../atoms/DclLogomark";
import "./gvdebugadmin.css";

const PANELS: { id: string; label: string }[] = [
  { id: "Admin", label: "Admin" },
  { id: "Debug", label: "Debug" },
];

const FUNCTION_NAME_OPTIONS = [
  "runQueuedAirdropJobs",
  "giveAndRevokeLandOwnerBadges",
  "giveTopVoterBadges",
  "restoreMissingUpdatesForumPost",
];

type SelectOption = { text: ReactNode; value: string | number };

const REVOKE_REASON_OPTIONS: SelectOption[] = [
  { text: "Abuse", value: "Abuse" },
  { text: "Left Community", value: "LeftCommunity" },
  { text: "Tenure Ended", value: "TenureEnded" },
  { text: "Other", value: "Other" },
];

const NOTIFICATION_TYPES: SelectOption[] = [
  { text: "Target", value: 0 },
  { text: "Broadcast", value: 1 },
];

function Heading({ size = "sm", children, className }: { size?: string; children?: ReactNode; className?: string }) {
  return <h2 className={"da__heading da__heading--" + size + (className ? " " + className : "")}>{children}</h2>;
}

function Label({ children, htmlFor }: { children?: ReactNode; htmlFor?: string }) {
  return <label className="da__label" htmlFor={htmlFor}>{children}</label>;
}

function SubLabel({ children }: { children?: ReactNode }) {
  return <p className="da__sublabel">{children}</p>;
}

function ResultText({ children }: { children?: ReactNode }) {
  return <p className="da__result">{children}</p>;
}

function Field({ id, value, placeholder, ariaLabel, disabled, onChange }: { id?: string; value: string; placeholder?: string; ariaLabel?: string; disabled?: boolean; onChange?: (value: string) => void }) {
  return (
    <input
      id={id}
      className="da__field"
      type="text"
      value={value}
      placeholder={placeholder}
      aria-label={ariaLabel}
      disabled={disabled}
      onChange={(e) => onChange?.(e.target.value)}
    />
  );
}

function SelectField({ id, value, options, ariaLabel, disabled, onChange }: { id?: string; value: string | number; options: SelectOption[]; ariaLabel?: string; disabled?: boolean; onChange?: (value: string) => void }) {
  return (
    <div className={"da__select" + (disabled ? " is-disabled" : "")}>
      <select id={id} value={value} aria-label={ariaLabel} disabled={disabled} onChange={(e) => onChange?.(e.target.value)}>
        {options.map((o) => (
          <option key={String(o.value)} value={o.value}>
            {o.text}
          </option>
        ))}
      </select>
      <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true" className="da__selectcaret">
        <path d="M4 6l4 4 4-4" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" strokeLinejoin="round" fill="none" />
      </svg>
    </div>
  );
}

function AddressesSelect({ values = [], disabled }: { values?: string[]; disabled?: boolean }) {
  return (
    <div className={"da__addresses" + (disabled ? " is-disabled" : "")}>
      {values.map((a) => (
        <span key={a} className="da__addrchip">
          <span className="da__addravatar u-avatar" style={{ "--sz": "18px", "--hue": 268 } as CSSProperties & { "--sz": string; "--hue": number }} aria-hidden="true" />
          {a}
          <button type="button" className="da__addrx" aria-label={"Remove " + a}>×</button>
        </span>
      ))}
      <input className="da__addrinput" type="text" placeholder="Add address (0x…)" aria-label="Add address" disabled={disabled} />
    </div>
  );
}

function Button({ children, className, primary = true, disabled, onClick, type = "button" }: { children?: ReactNode; className?: string; primary?: boolean; disabled?: boolean; onClick?: () => void; type?: "button" | "submit" | "reset" }) {
  return (
    <button
      type={type}
      className={"da__btn" + (primary ? " da__btn--primary" : " da__btn--basic") + (className ? " " + className : "")}
      disabled={disabled}
      onClick={onClick}
    >
      {children}
    </button>
  );
}

function ErrorMessage({ label, errorMessage }: { label: ReactNode; errorMessage: ReactNode }) {
  const [open, setOpen] = useState(false);
  return (
    <div className="da__error">
      <div className="da__errorhead">
        <span className="da__errornotice">
          <svg viewBox="0 0 24 24" width="18" height="18" aria-hidden="true" className="da__erroricon">
            <circle cx="12" cy="12" r="10" fill="currentColor" />
            <path d="M12 7v6M12 16.5v.5" stroke="#fff" strokeWidth="2" strokeLinecap="round" />
          </svg>
          <span className="da__errorlabel">{label}</span>
        </span>
        <button type="button" className="da__errortoggle" onClick={() => setOpen((o) => !o)}>
          {open ? "Hide" : "Show"}
        </button>
      </div>
      {open && (
        <div className="da__errorbody">
          <div className="da__errormsg">
            <pre>{errorMessage}</pre>
            <button type="button" className="da__errorcopy">Copy</button>
          </div>
          <p className="da__errorcfa">
            Try again later. If the problem persists, please report it on Discord.
          </p>
        </div>
      )}
    </div>
  );
}

function Section({ title, headingSize = "sm", children, error, className }: { title: ReactNode; headingSize?: string; children?: ReactNode; error?: { label: ReactNode; message: ReactNode }; className?: string }) {
  return (
    <div className={"da__section" + (className ? " " + className : "")}>
      <div className="da__content">
        <Heading size={headingSize}>{title}</Heading>
        {children}
      </div>
      {error && <ErrorMessage label={error.label} errorMessage={error.message} />}
    </div>
  );
}

function BudgetsUpdate() {
  return (
    <Section title="Budgets">
      <div className="da__btnrow">
        <Button className="Debug__SectionButton">Fetch Transparency Budgets</Button>
        <Button className="Debug__SideButton">Update Governance Budgets</Button>
      </div>
      <Label>Transparency Budgets</Label>
      <ResultText>
        {'[{"start":"2024-01-01","finish":"2024-03-31","total":1209090,"category_percentages":{"accelerator":7,"core_unit":15,"documentation":2}}]'}
      </ResultText>
    </Section>
  );
}

function BadgesAdmin() {
  return (
    <Section title="Badges" error={{ label: "Badges Error", message: "Error: badge spec cid not found on Otterspace registry" }}>
      <div>
        <Heading size="xs">Create, Airdrop, Revoke</Heading>
        <div className="da__btnrow">
          <Button className="Debug__SectionButton">Create Badge Spec</Button>
          <Button className="Debug__SideButton">Airdrop</Button>
          <Button className="Debug__SideButton">Revoke</Button>
        </div>
        <Label htmlFor="da-badge-cid">Badge Spec Cid</Label>
        <Field id="da-badge-cid" value="bafkreigh2akiscaildc..." onChange={() => {}} />
        <Label>Recipients</Label>
        <AddressesSelect values={["0x7c…a4e1", "buildersdao.dcl"]} />
        <Label htmlFor="da-revoke-reason">Revoke Reason</Label>
        <SelectField id="da-revoke-reason" value="TenureEnded" options={REVOKE_REASON_OPTIONS} onChange={() => {}} />
      </div>
      <div className="da__content da__subblock">
        <Heading size="xs">Upload Badge Spec</Heading>
        <Label htmlFor="da-badge-title">Title</Label>
        <Field id="da-badge-title" value="Top Voter Q2 2024" onChange={() => {}} />
        <Label htmlFor="da-badge-desc">Description</Label>
        <Field id="da-badge-desc" value="Awarded to the most active voters of the quarter." onChange={() => {}} />
        <Label htmlFor="da-badge-exp">Expiration Date</Label>
        <Field id="da-badge-exp" value="2025-06-30" onChange={() => {}} />
        <Label htmlFor="da-badge-img">Image Url</Label>
        <Field id="da-badge-img" value="https://badges.decentraland.org/top-voter.png" onChange={() => {}} />
      </div>
      <div className="da__btnrow">
        <Button className="Debug__SectionButton">Upload Spec</Button>
      </div>
      <Label>Result</Label>
      <SubLabel>Badge Cid</SubLabel>
      <ResultText>bafkreigh2akiscaildcoutsidedonut...</ResultText>
      <SubLabel>IPFS Address</SubLabel>
      <ResultText>ipfs://bafkreigh2akiscaildcoutsidedonut.../metadata.json</ResultText>
    </Section>
  );
}

function TriggerFunction() {
  return (
    <Section title="Trigger Function">
      <Label htmlFor="da-fn-name">Function Name</Label>
      <SelectField id="da-fn-name" value="runQueuedAirdropJobs" options={FUNCTION_NAME_OPTIONS.map((f) => ({ text: f, value: f }))} onChange={() => {}} />
      <div className="da__btnrow">
        <Button className="Debug__SectionButton">Trigger</Button>
      </div>
      <Label>Result</Label>
      <ResultText>{'{"queued":3,"processed":3,"failed":0}'}</ResultText>
    </Section>
  );
}

function Notifications() {
  return (
    <Section title="Send announcement notification">
      <SelectField value={0} options={NOTIFICATION_TYPES} ariaLabel="Notification type" onChange={() => {}} />
      <Field value="" placeholder="Address (0x...)" ariaLabel="Address" onChange={() => {}} />
      <Field value="" placeholder="Title" ariaLabel="Title" onChange={() => {}} />
      <Field value="" placeholder="Body" ariaLabel="Body" onChange={() => {}} />
      <Field value="" placeholder="URL" ariaLabel="URL" onChange={() => {}} />
      <div className="da__btnrow">
        <Button>Send</Button>
      </div>
    </Section>
  );
}

function InvalidateCache() {
  return (
    <Section title="Invalidate Cache">
      <div className="da__inline">
        <Field value="" placeholder="Cache key" ariaLabel="Cache key" onChange={() => {}} />
        <Button disabled>Remove key</Button>
      </div>
    </Section>
  );
}

function ErrorReporting() {
  return (
    <Section title="Report Error to Server">
      <Label htmlFor="da-err-msg">Error Message</Label>
      <Field id="da-err-msg" value="" onChange={() => {}} />
      <Label htmlFor="da-err-json">JSON data</Label>
      <Field id="da-err-json" value="{}" onChange={() => {}} />
      <div className="da__btnrow">
        <Button className="Debug__SectionButton" disabled>Report Error</Button>
      </div>
    </Section>
  );
}

function HttpStatus() {
  return (
    <Section title="HTTP Status" className="da__submit">
      <Label htmlFor="da-http-status">Http Status</Label>
      <Field id="da-http-status" value="" onChange={() => {}} />
      <Label htmlFor="da-http-sleep">Sleep</Label>
      <Field id="da-http-sleep" value="0" onChange={() => {}} />
      <div className="da__btnrow">
        <Button>Submit</Button>
      </div>
    </Section>
  );
}

function EnvStatus() {
  return (
    <Section title="Frontend Env Variables">
      <div className="da__inline EnvName__Section">
        <Field value="GOVERNANCE_API" ariaLabel="Env var name" onChange={() => {}} />
        <Button className="Debug__SideButton">Read Env Var</Button>
      </div>
      <Field value={'"https://governance.decentraland.org/api"'} ariaLabel="Env var value" disabled onChange={() => {}} />
    </Section>
  );
}

function Snapshot() {
  return (
    <div className="da__section">
      <Heading size="sm">Snapshot</Heading>
      <div className="da__content">
        <Label htmlFor="da-space-name">Space Name</Label>
        <div className="da__inline SpaceName__Section">
          <Field id="da-space-name" value="snapshot.dcl.eth" onChange={() => {}} />
          <Button className="Debug__SideButton">Fetch Status &amp; Space</Button>
        </div>
      </div>
      <Label>Config</Label>
      <ResultText>{'{"network":"1","strategies":[{"name":"erc20-balance-of"}],"plugins":{}}'}</ResultText>
      <Label>Space</Label>
      <ResultText>{'{"id":"snapshot.dcl.eth","name":"Decentraland","symbol":"VP","members":12}'}</ResultText>
    </div>
  );
}

function QueryData() {
  return (
    <Section title="Query Data">
      <div className="da__querybtns">
        <Button>Get All Events</Button>
        <Button>Get All Airdrop Jobs</Button>
      </div>
    </Section>
  );
}

function LogInGate() {
  return (
    <div className="da__login">
      <div className="da__loginbox">
        <DclLogomark size={56} className="da__loginlogo" />
        <h1 className="da__logintitle">Sign In</h1>
        <p className="da__loginsub">You need to sign in to access this page.</p>
        <button type="button" className="da__loginbtn">Connect</button>
      </div>
    </div>
  );
}

export default function GvDebugAdmin({ authorized = true, version = "v3.41.0" }: { authorized?: boolean; version?: string }) {
  const [tab, setTab] = useState<GovernanceNavId>("transparency");
  const [panel, setPanel] = useState("Admin");

  if (!authorized) {
    return (
      <GovernanceChrome active={tab} onTab={setTab}>
        <LogInGate />
      </GovernanceChrome>
    );
  }

  const isAdmin = panel === "Admin";

  return (
    <GovernanceChrome active={tab} onTab={setTab}>
      <div className="da">
        <Heading size="sm">Version</Heading>
        <p className="da__version">{version}</p>

        <div className="da__tabs">
          {PANELS.map((p) => (
            <button
              key={p.id}
              type="button"
              className={"da__tab" + (p.id === panel ? " is-active" : "")}
              aria-current={p.id === panel ? "true" : undefined}
              onClick={() => setPanel(p.id)}
            >
              {p.label}
            </button>
          ))}
        </div>

        {isAdmin ? (
          <>
            <BudgetsUpdate />
            <BadgesAdmin />
            <TriggerFunction />
            <Notifications />
          </>
        ) : (
          <>
            <InvalidateCache />
            <ErrorReporting />
            <HttpStatus />
            <EnvStatus />
            <Snapshot />
            <QueryData />
          </>
        )}
      </div>
    </GovernanceChrome>
  );
}
