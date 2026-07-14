import { useEffect, useMemo, useRef, useState } from "react";
import { useMachine } from "@xstate/react";
import { useSearchParams } from "react-router";

import GovernanceChrome, { type GovernanceNavId } from "@ui/governance/frames/GovernanceChrome";
import GvProposalDetailSuccessOutcomeScreens from "@ui/governance/components/GvProposalDetailSuccessOutcomeScreens";
import "@ui/governance/pages/gvsubmitgrant.css";

import type { TrackContext } from "@core/lib/telemetry/track";
import type { GrantBudget, GrantCategory, GrantTier } from "@data/lib/catalyst/governance/grant-budget";
import {
  grantMachine,
  resolveGrantSnapshot,
  slugToState,
  stateToSlug,
  type GrantDraft,
  type SubmitFn,
  type TrackFn,
} from "./machine";

export type SubmitGrantWizardProps = {
  budget: GrantBudget;
  trackCtx: TrackContext;
  initialStep?: string;
  submitGrant?: SubmitFn;
  track?: TrackFn;
};

const SECTION_TITLES = [
  "Funding",
  "General Information",
  "Team",
  "Due Diligence",
  "Category-Specific Assessment",
  "Final Consent",
] as const;

function tierForBudget(tiers: GrantTier[], budget: number): GrantTier | undefined {
  if (!budget) return undefined;
  return (
    tiers.find((t) => budget >= t.min && budget <= t.max) ??
    tiers[tiers.length - 1]
  );
}

function CategoryGlyph({ tone }: { tone: string }) {
  return (
    <span className={`gvsubmitgrant__catglyph gvsubmitgrant__catglyph--${tone}`} aria-hidden="true">
      <svg viewBox="0 0 24 24" width="22" height="22" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round">
        <path d="M3 7l9-4 9 4-9 4-9-4Z" />
        <path d="M3 7v6l9 4 9-4V7" />
        <path d="M12 11v10" />
      </svg>
    </span>
  );
}

function SectionIcon({ n, validated }: { n: number; validated: boolean }) {
  return (
    <span className="gvsubmitgrant__secicon" aria-hidden="true">
      <svg viewBox="0 0 34 42" width="34" height="42" fill="none">
        <circle cx="16" cy="21" r="15.5" stroke="#b4b0be" strokeOpacity="0.6" />
        <text x="16" y="26" textAnchor="middle" fontSize="15" fontWeight="600" fill="#fcfcfc">{n}</text>
        {validated && (
          <>
            <circle cx="27" cy="32" r="6" fill="#44b600" stroke="#fff" strokeWidth="2" />
            <path d="M25 32C26 33 26.5 33.5 26.5 33.5L29.5 30.5" stroke="#fff" strokeWidth="1.2" />
          </>
        )}
      </svg>
    </span>
  );
}

const Helper = () => (
  <span className="gvsubmitgrant__helper" aria-hidden="true">?</span>
);

function FormSection({
  n,
  title,
  validated,
  children,
}: {
  n: number;
  title: string;
  validated: boolean;
  children?: React.ReactNode;
}) {
  return (
    <section className="gvsubmitgrant__section">
      <div className="gvsubmitgrant__sectionhead">
        <SectionIcon n={n} validated={validated} />
        <span className="gvsubmitgrant__sectiontitle">{title}</span>
        <span className="gvsubmitgrant__rule" aria-hidden="true" />
      </div>
      {children && <div className="gvsubmitgrant__sectionbody">{children}</div>}
    </section>
  );
}

export default function SubmitGrantWizard({
  budget,
  trackCtx,
  initialStep,
  submitGrant,
  track,
}: SubmitGrantWizardProps) {
  const [searchParams] = useSearchParams();
  const urlStep = (searchParams.get("step")?.trim() || initialStep) ?? undefined;
  const stateId = slugToState(urlStep);

  return (
    <SubmitGrantWizardInner
      key={stateId}
      stateId={stateId}
      budget={budget}
      trackCtx={trackCtx}
      submitGrant={submitGrant}
      track={track}
    />
  );
}

type InnerProps = {
  stateId: ReturnType<typeof slugToState>;
  budget: GrantBudget;
  trackCtx: TrackContext;
  submitGrant?: SubmitFn;
  track?: TrackFn;
};

function SubmitGrantWizardInner({
  stateId,
  budget,
  trackCtx,
  submitGrant,
  track,
}: InnerProps) {
  const [, setSearchParams] = useSearchParams();
  const [tab, setTab] = useState<GovernanceNavId>("projects");

  const seededDraft: Partial<GrantDraft> | undefined =
    stateId === "category" ? undefined : { category: "Platform", budget: 24000, duration: 6 };

  const snapshot = useRef(
    resolveGrantSnapshot({ step: stateId, trackCtx, submitGrant, track, draft: seededDraft }),
  ).current;

  const [state, send] = useMachine(grantMachine, {
    input: { trackCtx, submitGrant, track, draft: seededDraft },
    snapshot,
  });

  const value = state.value as string;
  const step = stateToSlug(value);
  const draft = state.context.draft;

  const [fundingInput, setFundingInput] = useState<string>(
    draft.budget ? String(draft.budget) : "",
  );
  const [duration, setDuration] = useState<number>(draft.duration || 3);

  const numericBudget = Number(fundingInput) || 0;
  const tier = useMemo(
    () => tierForBudget(budget.tiers, numericBudget),
    [budget.tiers, numericBudget],
  );

  const selectedCategory: GrantCategory | undefined = useMemo(
    () => budget.categories.find((c) => c.id === draft.category),
    [budget.categories, draft.category],
  );

  const lastStep = useRef<string | null>(null);
  useEffect(() => {
    if (lastStep.current === step) return;
    lastStep.current = step;
    setSearchParams(
      (prev) => {
        const params = new URLSearchParams(prev);
        if (params.get("step") === step) return params;
        params.set("step", step);
        return params;
      },
      { replace: true, preventScrollReset: true },
    );
  }, [step, setSearchParams]);

  const validCategories = budget.categories.filter((c) => !c.suspended);
  const suspendedCategories = budget.categories.filter((c) => c.suspended);

  return (
    <GovernanceChrome active={tab} onTab={setTab}>
      <div className="gvsubmitgrant" data-step={step} data-source={budget.source}>
        <div className="gvsubmitgrant__head">
          <div className="gvsubmitgrant__headleft">
            <h1 className="gvsubmitgrant__h1">Request a Grant</h1>
          </div>
          <button type="button" className="gvsubmitgrant__cancel">Cancel</button>
        </div>

        <p className="gvsubmitgrant__desc">
          The Decentraland Grants program allocates MANA owned by the DAO to fund
          the creation of features or content beneficial to the Decentraland
          platform and its growth. Either individuals or teams may request grant
          funding through the DAO.
        </p>
        <p className="gvsubmitgrant__desc gvsubmitgrant__desc--fine">
          This action requires at least {budget.submissionThresholdVp} VP. Buy MANA
          to get VP, or run for delegate.
        </p>

        {value === "category" && (
          <div className="gvsubmitgrant__selector">
            <p className="gvsubmitgrant__selectordesc">
              Choose a category that best suits your project
            </p>
            <div className="gvsubmitgrant__catgrid">
              {validCategories.map((c) => (
                <button
                  key={c.key}
                  type="button"
                  className={`gvsubmitgrant__cat gvsubmitgrant__cat--${c.tone}`}
                  onClick={() => send({ type: "PICK_CATEGORY", category: c.id })}
                >
                  <CategoryGlyph tone={c.tone} />
                  <span className="gvsubmitgrant__catcontent">
                    <span className="gvsubmitgrant__catname">{c.id}</span>
                    <span className="gvsubmitgrant__catdesc">{c.desc}</span>
                  </span>
                </button>
              ))}
            </div>

            {suspendedCategories.length > 0 && (
              <>
                <p className="gvsubmitgrant__suspended">
                  The budget allocation for the following Grant categories has been
                  suspended for the time being as a result of a binding community
                  decision.
                </p>
                <div className="gvsubmitgrant__catgrid">
                  {suspendedCategories.map((c) => (
                    <button
                      key={c.key}
                      type="button"
                      className="gvsubmitgrant__cat is-disabled"
                      disabled
                    >
                      <CategoryGlyph tone="neutral" />
                      <span className="gvsubmitgrant__catcontent">
                        <span className="gvsubmitgrant__catname">{c.id}</span>
                        <span className="gvsubmitgrant__catdesc">{c.desc}</span>
                      </span>
                    </button>
                  ))}
                </div>
              </>
            )}

            <p className="gvsubmitgrant__doc">
              Remaining budget for {budget.period.label} read live from the DAO
              budget API. Not sure which one to choose? Find out more about our
              Categories on our Grants Documentation.
            </p>
          </div>
        )}

        {value === "funding" && (
          <div className="gvsubmitgrant__form">
            <FormSection n={1} title="Funding" validated={!!numericBudget}>
              <div className="gvsubmitgrant__cardrow">
                <div className="gvsubmitgrant__sectioncard">
                  <div className="gvsubmitgrant__sccardhead">
                    <span className="gvsubmitgrant__sccardtitle">Project Category</span>
                    <button
                      type="button"
                      className="gvsubmitgrant__cardaction"
                      onClick={() => send({ type: "BACK" })}
                    >
                      Change
                    </button>
                  </div>
                  <div className="gvsubmitgrant__sccardcontent">
                    <span className="gvsubmitgrant__sccardvalue">{draft.category}</span>
                  </div>
                  <div className="gvsubmitgrant__sccardsub">
                    {selectedCategory?.desc ?? "Selected grant category"}
                  </div>
                </div>
                <div className="gvsubmitgrant__sectioncard">
                  <div className="gvsubmitgrant__sccardhead">
                    <span className="gvsubmitgrant__sccardtitle">Remaining Category Budget</span>
                    <Helper />
                  </div>
                  <div className="gvsubmitgrant__sccardcontent">
                    <span className="gvsubmitgrant__sccardvalue">
                      {selectedCategory?.availableLabel ?? "\u{2014}"}
                    </span>
                    {selectedCategory && (
                      <span className="gvsubmitgrant__sccardextra">
                        ({selectedCategory.availablePct}%)
                      </span>
                    )}
                  </div>
                  <div className="gvsubmitgrant__sccardcapsub">
                    Category total for {budget.period.label}:{" "}
                    {selectedCategory?.totalLabel ?? budget.period.totalLabel}
                  </div>
                </div>
              </div>

              <div className="gvsubmitgrant__cardrow">
                <div className="gvsubmitgrant__field">
                  <span className="gvsubmitgrant__label">Desired Funding</span>
                  <div className="gvsubmitgrant__budget">
                    <span className="gvsubmitgrant__budgetusd">USD</span>
                    <input
                      className="gvsubmitgrant__budgetinput"
                      type="number"
                      placeholder="100-240000"
                      value={fundingInput}
                      onChange={(e) => setFundingInput(e.target.value)}
                    />
                  </div>
                  <span className="gvsubmitgrant__sub">
                    {tier ? `${tier.id} \u{B7} ${tier.payout}` : "More about our Funding Tiers"}
                  </span>
                </div>
                <div className="gvsubmitgrant__field">
                  <span className="gvsubmitgrant__label">Estimated Project Duration</span>
                  <div className="gvsubmitgrant__stepper">
                    <button
                      type="button"
                      className="gvsubmitgrant__stepbtn"
                      onClick={() => setDuration((d) => Math.max(1, d - 1))}
                    >
                      &#x2212;
                    </button>
                    <span className="gvsubmitgrant__stepval">{duration} months</span>
                    <button
                      type="button"
                      className="gvsubmitgrant__stepbtn"
                      onClick={() => setDuration((d) => Math.min(12, d + 1))}
                    >
                      +
                    </button>
                  </div>
                </div>
              </div>

              <div className="gvsubmitgrant__cardrow">
                <div className="gvsubmitgrant__sectioncard">
                  <div className="gvsubmitgrant__sccardhead">
                    <span className="gvsubmitgrant__sccardtitle">Pass Threshold</span>
                    <Helper />
                  </div>
                  <div className="gvsubmitgrant__sccardcontent">
                    <span className="gvsubmitgrant__sccardvalue">
                      {tier ? `${tier.passThreshold} in 14 days` : "\u{2014}"}
                    </span>
                  </div>
                  <div className="gvsubmitgrant__sccardsub">
                    Accumulated VP needed for this proposal to pass
                  </div>
                </div>
                <div className="gvsubmitgrant__sectioncard">
                  <div className="gvsubmitgrant__sccardhead">
                    <span className="gvsubmitgrant__sccardtitle">Payout Strategy</span>
                    <Helper />
                  </div>
                  <div className="gvsubmitgrant__sccardcontent">
                    <span className="gvsubmitgrant__sccardvalue">
                      {numericBudget ? `${duration} Monthly Payments` : "\u{2014}"}
                    </span>
                  </div>
                  <div className="gvsubmitgrant__sccardsub">
                    Executed 7 days after the proposal has passed
                  </div>
                </div>
              </div>
            </FormSection>

            <div className="gvsubmitgrant__submitrow">
              <button
                type="button"
                className="gvsubmitgrant__cardaction"
                onClick={() => send({ type: "BACK" })}
              >
                Back
              </button>
              <button
                type="button"
                className="gvsubmitgrant__submit"
                disabled={!numericBudget}
                onClick={() =>
                  send({
                    type: "SET_FUNDING",
                    budget: numericBudget,
                    duration,
                    tier: tier?.id,
                  })
                }
              >
                Continue
              </button>
            </div>
          </div>
        )}

        {value === "general" && (
          <div className="gvsubmitgrant__form">
            <FormSection n={2} title="General Information" validated={false}>
              <div className="gvsubmitgrant__field">
                <span className="gvsubmitgrant__label">Project name</span>
                <div className="gvsubmitgrant__budget">
                  <input className="gvsubmitgrant__budgetinput" type="text" placeholder="Give your project a clear title" />
                </div>
              </div>
              <div className="gvsubmitgrant__field">
                <span className="gvsubmitgrant__label">Abstract (Markdown)</span>
                <span className="gvsubmitgrant__sub">A short summary of what you intend to build.</span>
              </div>
            </FormSection>
            <FormSection n={3} title="Team" validated={false}>
              <div className="gvsubmitgrant__field">
                <span className="gvsubmitgrant__label">Team members & roles</span>
              </div>
            </FormSection>
            <FormSection n={4} title="Due Diligence" validated={false}>
              <div className="gvsubmitgrant__field">
                <span className="gvsubmitgrant__label">Roadmap & milestones (Markdown)</span>
              </div>
            </FormSection>
            <div className="gvsubmitgrant__submitrow">
              <button type="button" className="gvsubmitgrant__cardaction" onClick={() => send({ type: "BACK" })}>
                Back
              </button>
              <button type="button" className="gvsubmitgrant__submit" onClick={() => send({ type: "NEXT" })}>
                Continue
              </button>
            </div>
          </div>
        )}

        {value === "assessment" && (
          <div className="gvsubmitgrant__form">
            <FormSection n={5} title={`Category-Specific Assessment \u{2014} ${draft.category}`} validated={false}>
              <div className="gvsubmitgrant__field">
                <span className="gvsubmitgrant__label">
                  Describe how this {draft.category} grant delivers measurable value
                </span>
              </div>
            </FormSection>
            <FormSection n={6} title="Final Consent" validated={false}>
              <label className="gvsubmitgrant__radio">
                <input type="radio" name="gvsg-consent" defaultChecked />
                <span className="gvsubmitgrant__radiomark" aria-hidden="true" />
                I understand the grant terms and accept the DAO's conditions.
              </label>
            </FormSection>
            <div className="gvsubmitgrant__submitrow">
              <button type="button" className="gvsubmitgrant__cardaction" onClick={() => send({ type: "BACK" })}>
                Back
              </button>
              <button type="button" className="gvsubmitgrant__submit" onClick={() => send({ type: "NEXT" })}>
                Continue
              </button>
            </div>
          </div>
        )}

        {value === "review" && (
          <div className="gvsubmitgrant__form">
            <FormSection n={1} title="Review your request" validated>
              <div className="gvsubmitgrant__cardrow">
                <div className="gvsubmitgrant__sectioncard">
                  <div className="gvsubmitgrant__sccardhead">
                    <span className="gvsubmitgrant__sccardtitle">Category</span>
                  </div>
                  <div className="gvsubmitgrant__sccardcontent">
                    <span className="gvsubmitgrant__sccardvalue">{draft.category}</span>
                  </div>
                  <div className="gvsubmitgrant__sccardsub">{selectedCategory?.desc}</div>
                </div>
                <div className="gvsubmitgrant__sectioncard">
                  <div className="gvsubmitgrant__sccardhead">
                    <span className="gvsubmitgrant__sccardtitle">Funding</span>
                  </div>
                  <div className="gvsubmitgrant__sccardcontent">
                    <span className="gvsubmitgrant__sccardvalue">
                      ${draft.budget.toLocaleString("en-US")}
                    </span>
                    {draft.tier && (
                      <span className="gvsubmitgrant__sccardextra">{draft.tier}</span>
                    )}
                  </div>
                  <div className="gvsubmitgrant__sccardsub">
                    {draft.duration} months &#xB7; paid out after the proposal passes
                  </div>
                </div>
              </div>
            </FormSection>
            <div className="gvsubmitgrant__submitrow">
              <button type="button" className="gvsubmitgrant__cardaction" onClick={() => send({ type: "BACK" })}>
                Back
              </button>
              <button type="button" className="gvsubmitgrant__submit" onClick={() => send({ type: "SUBMIT" })}>
                Submit grant request
              </button>
            </div>
          </div>
        )}

        {value === "submitting" && (
          <div className="gvsubmitgrant__form" aria-busy="true">
            <FormSection n={1} title={"Submitting your grant request\u{2026}"} validated={false}>
              <p className="gvsubmitgrant__desc">
                Publishing your proposal to the DAO. This step is{" "}
                <strong>simulated</strong> &#x2014; governance has no write API here.
              </p>
            </FormSection>
          </div>
        )}

        {value === "submitError" && (
          <div className="gvsubmitgrant__form">
            <FormSection n={1} title="Couldn't submit your grant request" validated={false}>
              <p className="gvsubmitgrant__desc gvsubmitgrant__desc--fine" role="alert">
                {state.context.error ?? "Submission failed."}
              </p>
            </FormSection>
            <div className="gvsubmitgrant__submitrow">
              <button type="button" className="gvsubmitgrant__cardaction" onClick={() => send({ type: "BACK" })}>
                Back to review
              </button>
              <button type="button" className="gvsubmitgrant__submit" onClick={() => send({ type: "RETRY" })}>
                Retry
              </button>
            </div>
          </div>
        )}
      </div>

      {value === "success" && (
        <GvProposalDetailSuccessOutcomeScreens variant="new" />
      )}
    </GovernanceChrome>
  );
}

export const GRANT_SECTION_TITLES = SECTION_TITLES;
