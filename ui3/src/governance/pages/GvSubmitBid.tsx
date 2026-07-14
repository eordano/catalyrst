import type { ReactNode } from "react";
import { useMemo, useState } from "react";
import GovernanceChrome from "../frames/GovernanceChrome";
import Modal from "../../components/Modal";
import "./gvsubmitbid.css";

type ParentProposal = {
  title: string;
  votingPower: string;
  finishAt: string;
};

const PITCH_PROPOSAL: ParentProposal = {
  title: "Improve the new-user onboarding funnel in Genesis City",
  votingPower: "1,284,902",
  finishAt: "2 months ago",
};
const TENDER_PROPOSAL: ParentProposal = {
  title: "Tender: scope a guided first-session experience for new wallets",
  votingPower: "942,118",
  finishAt: "26 days ago",
};

function SectionIcon({ sectionNumber, validated }: { sectionNumber: number; validated: boolean }) {
  return (
    <svg
      width="34"
      height="42"
      viewBox="0 0 34 42"
      fill="none"
      xmlns="http://www.w3.org/2000/svg"
      className="gvsubmitbid__sectionicon"
      aria-hidden="true"
    >
      <circle cx="16" cy="21" r="15.5" stroke="#736E7D" strokeOpacity="0.32" />
      <text
        x="16"
        y="21"
        textAnchor="middle"
        dominantBaseline="central"
        fontSize="15"
        fontWeight="600"
        fill="var(--gv-ink)"
      >
        {sectionNumber}
      </text>
      {validated && (
        <>
          <circle cx="27" cy="32" r="6" fill="#44B600" stroke="white" strokeWidth="2" />
          <path d="M25 32C26 33 26.5 33.5 26.5 33.5L29.5 30.5" stroke="white" />
        </>
      )}
    </svg>
  );
}

function Section({ number, title, validated = false, children }: { number: number; title: ReactNode; validated?: boolean; children?: ReactNode }) {
  return (
    <section className="gvsubmitbid__section">
      <div className="gvsubmitbid__sectionhead">
        <SectionIcon sectionNumber={number} validated={validated} />
        <h2 className="gvsubmitbid__sectiontitle">{title}</h2>
        <span className="gvsubmitbid__rule" />
      </div>
      <div className="gvsubmitbid__sectioncontent">{children}</div>
    </section>
  );
}

function Field({ label, labelFor, sublabel, message, postlabel, children }: { label?: ReactNode; labelFor?: string; sublabel?: ReactNode; message?: ReactNode; postlabel?: ReactNode; children?: ReactNode }) {
  return (
    <div className="gvsubmitbid__field">
      {label && (labelFor ? (
        <label className="gvsubmitbid__label" htmlFor={labelFor}>{label}</label>
      ) : (
        <span className="gvsubmitbid__label">{label}</span>
      ))}
      {sublabel && <span className="gvsubmitbid__sublabel">{sublabel}</span>}
      {children}
      {postlabel && <span className="gvsubmitbid__postlabel">{postlabel}</span>}
      {message && <span className="gvsubmitbid__message">{message}</span>}
    </div>
  );
}

function BudgetInput({ label, value, onChange, placeholder }: { label: ReactNode; value: string; onChange: (value: string) => void; placeholder?: string }) {
  return (
    <div className="gvsubmitbid__budget">
      <label className="gvsubmitbid__label" htmlFor="gvb-budget">{label}</label>
      <div className="gvsubmitbid__budgetbox">
        <span className="gvsubmitbid__budgetunit">USD</span>
        <input
          id="gvb-budget"
          className="gvsubmitbid__budgetinput"
          type="number"
          value={value}
          placeholder={placeholder}
          onChange={(e) => onChange(e.target.value)}
        />
      </div>
    </div>
  );
}

function NumberSelector({ label, value, unit, min, max, onChange }: { label: ReactNode; value: number; unit: ReactNode; min: number; max: number; onChange: (value: number) => void }) {
  return (
    <div className="gvsubmitbid__numsel">
      <span className="gvsubmitbid__label">{label}</span>
      <div className="gvsubmitbid__numselbox">
        <div className="gvsubmitbid__numselinput">
          <button
            type="button"
            className="gvsubmitbid__numbtn"
            onClick={() => onChange(Math.max(min, value - 1))}
            disabled={value <= min}
            aria-label="Decrease"
          >
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M3 8h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>
          </button>
          <span className="gvsubmitbid__numval">{value}</span>
          <button
            type="button"
            className="gvsubmitbid__numbtn"
            onClick={() => onChange(Math.min(max, value + 1))}
            disabled={value >= max}
            aria-label="Increase"
          >
            <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true"><path d="M8 3v10M3 8h10" stroke="currentColor" strokeWidth="1.6" strokeLinecap="round" /></svg>
          </button>
        </div>
        <div className="gvsubmitbid__numselunit">{unit}</div>
      </div>
    </div>
  );
}

function ParentCard({ kind, proposal }: { kind: ReactNode; proposal: ParentProposal }) {
  return (
    <button type="button" className="gvsubmitbid__parentcard">
      <div className="gvsubmitbid__parenthead">
        <span className="gvsubmitbid__parentkind">{kind}</span>
        <svg viewBox="0 0 16 16" width="14" height="14" className="gvsubmitbid__extlink" aria-hidden="true">
          <path d="M6 3h7v7M13 3 5 11M11 9v3H3V4h3" stroke="currentColor" strokeWidth="1.4" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
      </div>
      <div className="gvsubmitbid__parenttitle">{proposal.title}</div>
      <div className="gvsubmitbid__parentsub">
        <svg viewBox="0 0 24 24" width="14" height="14" className="gvsubmitbid__check" aria-hidden="true">
          <circle cx="12" cy="12" r="9" stroke="currentColor" strokeWidth="1.8" fill="none" />
          <path d="M8 12.5 11 15.5 16 9" stroke="currentColor" strokeWidth="1.8" fill="none" strokeLinecap="round" strokeLinejoin="round" />
        </svg>
        <span>
          Passed with <strong>{proposal.votingPower} VP</strong> {proposal.finishAt}
        </span>
      </div>
    </button>
  );
}

function AddBox({ children, onClick }: { children?: ReactNode; onClick?: () => void }) {
  return (
    <button type="button" className="gvsubmitbid__addbox" onClick={onClick}>
      <svg viewBox="0 0 24 24" width="18" height="18" className="gvsubmitbid__addicon" aria-hidden="true">
        <path d="M12 5v14M5 12h14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
      </svg>
      <span>{children}</span>
    </button>
  );
}

function BreakdownItem({ title, subtitle, onClick }: { title: ReactNode; subtitle: ReactNode; onClick?: () => void }) {
  return (
    <button type="button" className="gvsubmitbid__breakdown" onClick={onClick}>
      <div className="gvsubmitbid__breakdowntext">
        <p className="gvsubmitbid__breakdowntitle">{title}</p>
        <p className="gvsubmitbid__breakdownsub">{subtitle}</p>
      </div>
      <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" className="gvsubmitbid__chevron">
        <path d="M6 4l4 4-4 4" stroke="currentColor" strokeWidth="1.6" fill="none" strokeLinecap="round" strokeLinejoin="round" />
      </svg>
    </button>
  );
}

function CheckboxField({ checked, onToggle, children }: { checked: boolean; onToggle: () => void; children?: ReactNode }) {
  return (
    <button
      type="button"
      className={"gvsubmitbid__consent" + (checked ? " is-checked" : "")}
      onClick={onToggle}
      role="checkbox"
      aria-checked={checked}
    >
      <span className="gvsubmitbid__checkbox">
        {checked && (
          <svg viewBox="0 0 16 16" width="12" height="12" aria-hidden="true">
            <path d="M3 8.5 6.5 12 13 4.5" stroke="currentColor" strokeWidth="2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
          </svg>
        )}
      </span>
      <span className="gvsubmitbid__consenttext">{children}</span>
    </button>
  );
}

const FUNDING_MIN = 100;
const FUNDING_MAX = 240000;
const DURATION_MIN = 1;
const DURATION_MAX = 12;

type Milestone = { title: string; tasks: string };
type Member = { name: string; role: string };
type BreakdownRow = { concept: string; amount: string };
type Consent = { contentPolicy: boolean; termsOfUse: boolean; codeOfEthics: boolean };

export default function GvSubmitBid({ submitted = false }: { submitted?: boolean }) {
  const [funding, setFunding] = useState("");
  const [duration, setDuration] = useState(DURATION_MIN);
  const [deliveryDate, setDeliveryDate] = useState("");
  const [beneficiary, setBeneficiary] = useState("");
  const [email, setEmail] = useState("");
  const [teamName, setTeamName] = useState("");
  const [deliverables, setDeliverables] = useState("");
  const [roadmap, setRoadmap] = useState("");
  const [milestones, setMilestones] = useState<Milestone[]>([
    { title: "2026-08-01 - Discovery & research", tasks: "User interviews, funnel analytics, scope lock" },
  ]);
  const [members, setMembers] = useState<Member[]>([
    { name: "ada.dcl", role: "Lead engineer" },
  ]);
  const [breakdown, setBreakdown] = useState<BreakdownRow[]>([
    { concept: "Engineering (3 devs)", amount: "$90,000 \u{B7} 4 months" },
  ]);
  const [consent, setConsent] = useState<Consent>({
    contentPolicy: false,
    termsOfUse: false,
    codeOfEthics: false,
  });

  const consentAll = consent.contentPolicy && consent.termsOfUse && consent.codeOfEthics;
  const allValid = useMemo(
    () =>
      Number(funding) >= FUNDING_MIN &&
      deliveryDate &&
      beneficiary &&
      email &&
      teamName.length >= 1 &&
      deliverables.length >= 20 &&
      roadmap.length >= 20 &&
      members.length > 0 &&
      consentAll,
    [funding, deliveryDate, beneficiary, email, teamName, deliverables, roadmap, members, consentAll]
  );

  return (
    <GovernanceChrome active="proposals">
      <div className="gvsubmitbid">
        <header className="gvsubmitbid__head">
          <div className="gvsubmitbid__headleft">
            <h1 className="gvsubmitbid__title">Submit Bid proposal</h1>
          </div>
          <button type="button" className="gvsubmitbid__cancel">Cancel</button>
        </header>

        <p className="gvsubmitbid__description">
          Part of the Bidding &amp; Tendering process, bid proposals are meant for
          professional teams to scope and propose a project out of their own
          understanding of one given issue or desire outlined by the Community based
          on the two preceding instances that would have passed. Only one Bid
          Proposal per a given Tender Proposal will be funded.
        </p>

        <div className="gvsubmitbid__parents">
          <ParentCard kind="Parent Pitch" proposal={PITCH_PROPOSAL} />
          <ParentCard kind="Parent Tender" proposal={TENDER_PROPOSAL} />
        </div>

        <Section number={1} title="Funding" validated={Number(funding) >= FUNDING_MIN && !!deliveryDate && !!beneficiary && !!email}>
          <div className="gvsubmitbid__row">
            <BudgetInput
              label="Budget"
              value={funding}
              onChange={setFunding}
              placeholder={`${FUNDING_MIN}-${FUNDING_MAX}`}
            />
            <NumberSelector
              label="Project duration"
              value={duration}
              unit="months"
              min={DURATION_MIN}
              max={DURATION_MAX}
              onChange={setDuration}
            />
          </div>
          <Field label="Delivery date" labelFor="gvb-delivery">
            <input
              id="gvb-delivery"
              className="gvsubmitbid__input"
              type="date"
              value={deliveryDate}
              onChange={(e) => setDeliveryDate(e.target.value)}
            />
          </Field>
          <Field
            label="Beneficiary address"
            labelFor="gvb-beneficiary"
            sublabel="The address that will receive the grant funds. This must be an Ethereum address! Entering a non-Ethereum address that cannot receive MANA may result in a permanent loss of funds."
          >
            <input
              id="gvb-beneficiary"
              className="gvsubmitbid__input"
              type="text"
              placeholder={"0x\u{2026}"}
              value={beneficiary}
              onChange={(e) => setBeneficiary(e.target.value)}
            />
          </Field>
          <Field
            label="Contact Email Address"
            labelFor="gvb-email"
            sublabel="This email address will be used by the Grant Support teams to contact you to check the progress of the grant, set up meetings, and maintain an open communication channel."
            postlabel="Note: The address will be published in the proposal and publicly visible. If you want to keep your anonymity consider using an email address without personally identifiable information."
          >
            <input
              id="gvb-email"
              className="gvsubmitbid__input"
              type="email"
              placeholder="Enter your email address"
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
          </Field>
        </Section>

        <Section
          number={2}
          title="General information"
          validated={teamName.length >= 1 && deliverables.length >= 20 && roadmap.length >= 20}
        >
          <Field
            label="Team Name"
            labelFor="gvb-team"
            message={`(${teamName.length} out of 80 characters)`}
          >
            <input
              id="gvb-team"
              className="gvsubmitbid__input"
              type="text"
              value={teamName}
              onChange={(e) => setTeamName(e.target.value.slice(0, 80))}
            />
          </Field>
          <Field
            label="Deliverables"
            labelFor="gvb-deliverables"
            sublabel="Be as specific as possible. Describe the entire scope of the project and the actual work you are going to deliver to the DAO."
            message={`(${deliverables.length} out of 1500 characters)`}
          >
            <textarea
              id="gvb-deliverables"
              className="gvsubmitbid__textarea"
              rows={5}
              value={deliverables}
              onChange={(e) => setDeliverables(e.target.value.slice(0, 1500))}
            />
          </Field>
          <Field
            label="Roadmap"
            labelFor="gvb-roadmap"
            sublabel="Describe the main phases or steps your project will follow to reach its goal."
            message={`(${roadmap.length} out of 1500 characters)`}
          >
            <textarea
              id="gvb-roadmap"
              className="gvsubmitbid__textarea"
              rows={5}
              placeholder="Your estimated timeline and key milestones. Include your plan for reporting progress to the community."
              value={roadmap}
              onChange={(e) => setRoadmap(e.target.value.slice(0, 1500))}
            />
          </Field>
          <Field
            label="Milestones"
            sublabel="Identify the important goals or checkpoints you want to achieve during your project."
          >
            {milestones.map((m, i) => (
              <BreakdownItem key={i} title={m.title} subtitle={m.tasks} onClick={() => {}} />
            ))}
            <AddBox onClick={() => setMilestones((s) => [...s, { title: "2026-09-15 - Beta release", tasks: "QA, instrumentation, launch" }])}>
              Add milestone
            </AddBox>
          </Field>
        </Section>

        <Section number={3} title="Team" validated={members.length > 0}>
          <Field
            label="Members"
            sublabel={"Please list who will be working on this project and include an explicit overview of their relevant skillset and experience. You may provide links to portfolios or profiles to help the Decentraland community get to know who the DAO will be funding and how their backgrounds will contribute to your project\u{2019}s success."}
          >
            {members.map((m, i) => (
              <BreakdownItem key={i} title={m.name} subtitle={m.role} onClick={() => {}} />
            ))}
            <AddBox onClick={() => setMembers((s) => [...s, { name: "grace.dcl", role: "Designer" }])}>
              Add team member
            </AddBox>
          </Field>
        </Section>

        <Section number={4} title="Due-diligence" validated={breakdown.length > 0}>
          <Field
            label="Budget breakdown"
            sublabel="Please provide a detailed specification on how you will be using the funds requested for this Grant. Our community values transparency, so be as specific as possible."
          >
            {breakdown.map((b, i) => (
              <BreakdownItem key={i} title={b.concept} subtitle={b.amount} onClick={() => {}} />
            ))}
            <AddBox onClick={() => setBreakdown((s) => [...s, { concept: "Marketing", amount: "$12,000 \u{B7} 2 months" }])}>
              Add concept
            </AddBox>
          </Field>
        </Section>

        <Section number={5} title="Final Consent" validated={consentAll}>
          <span className="gvsubmitbid__label">Review and check the following</span>
          <CheckboxField
            checked={consent.contentPolicy}
            onToggle={() => setConsent((c) => ({ ...c, contentPolicy: !c.contentPolicy }))}
          >
            I&#x2019;ve read and understood Decentraland&#x2019;s Content Policy
          </CheckboxField>
          <CheckboxField
            checked={consent.termsOfUse}
            onToggle={() => setConsent((c) => ({ ...c, termsOfUse: !c.termsOfUse }))}
          >
            I&#x2019;ve read Decentraland&#x2019;s Terms of Use and agree with them
          </CheckboxField>
          <CheckboxField
            checked={consent.codeOfEthics}
            onToggle={() => setConsent((c) => ({ ...c, codeOfEthics: !c.codeOfEthics }))}
          >
            I&#x2019;ve read Decentraland&#x2019;s Code of Ethics and hereby commit to honoring it
          </CheckboxField>
        </Section>

        <div className="gvsubmitbid__submit">
          <button type="button" className="gvsubmitbid__submitbtn" disabled={!allValid}>
            Submit proposal
          </button>
        </div>

        {submitted && (
          <Modal width={420} className="modal__card--plain gvsubmitbid__modal" ariaLabel="Bid submitted">
              <div className="gvsubmitbid__modalmark">
                <svg viewBox="0 0 24 24" width="34" height="34" aria-hidden="true">
                  <circle cx="12" cy="12" r="11" fill="#44B600" />
                  <path d="M7 12.5 10.5 16 17 8.5" stroke="white" strokeWidth="2.2" fill="none" strokeLinecap="round" strokeLinejoin="round" />
                </svg>
              </div>
              <h3 className="gvsubmitbid__modaltitle">Your bid was submitted!</h3>
              <p className="gvsubmitbid__modaltext">
                Your Bid proposal has been published against the parent Tender. The
                community can now review and vote on it.
              </p>
              <button type="button" className="gvsubmitbid__modalbtn">View proposal</button>
          </Modal>
        )}
      </div>
    </GovernanceChrome>
  );
}
