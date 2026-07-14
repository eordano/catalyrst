import EmptyState from "../../components/EmptyState";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import { groupDigits, usd, type FdPricingVM, type FdUsageVM } from "./FdCostsPage";
import "./fdcopilot.css";

export type FdCopilotStatusVM = {
  online: boolean;
  /** Whether the service looks provisioned at all — so "offline" can tell a
   *  failed probe of a live service apart from a service that was never deployed. */
  deployed?: boolean;
  probedAt: string;
  version?: string;
};

export type FdSkillVM = {
  name: string;
  description: string;
  dir: string;
  firstCommit: string | null;
  lastCommit: string | null;
  source: "sdk-skills" | "pre-prod";
};

export type FdCatalogSourceVM = {
  readAt: string;
  note: string;
};

/** Dates are git commit stamps; only the day is meaningful for a skill library. */
function day(iso: string | null): string {
  if (!iso) return "—";
  const d = new Date(iso);
  return Number.isNaN(d.getTime()) ? iso : d.toISOString().slice(0, 10);
}

export type FdCopilotPageProps = {
  status: FdCopilotStatusVM;
  usage: FdUsageVM;
  usageUnavailable?: boolean;
  skills: readonly FdSkillVM[];
  catalogSource?: FdCatalogSourceVM;
  /** null when the web interface has no published URL yet (backend can still be
   *  up on its socket — the CTA then reads "interface pending" rather than
   *  linking somewhere that does not answer). */
  openUrl: string | null;
  pricing: FdPricingVM;
  onOpen?: () => void;
  onSkillLink?: (skill: string) => void;
};

export default function FdCopilotPage({
  status,
  usage,
  usageUnavailable = false,
  skills,
  catalogSource,
  openUrl,
  pricing,
  onOpen,
  onSkillLink,
}: FdCopilotPageProps) {
  const sdk = skills.filter((s) => s.source === "sdk-skills");
  const preProd = skills.filter((s) => s.source === "pre-prod");
  const notDeployed = !status.online && status.deployed === false;
  const pillText = status.online
    ? "Copilot online"
    : notDeployed
      ? "Copilot not deployed — no copilot service is provisioned on this server"
      : "Copilot offline — the service is deployed but did not answer the probe just now";

  return (
    <div className="fd-page fd-stack fd-copilot">
      <FdPageHead
        title="The copilot"
        intro="A self-hosted opencode instance wired to our own model gateway. Conversation happens in its own interface — this page reports whether it is up, what it has spent, and which of its skills work today."
        aside={
          <span
            className={
              "fd-copilot__pill" + (status.online ? " is-online" : " is-offline")
            }
          >
            {pillText}
          </span>
        }
      />

      <FdSection title="Open it">
        <p className="fd-copilot__cta">
          {status.online && openUrl ? (
            <a
              className="fd-copilot__open"
              href={openUrl}
              rel="noreferrer"
              onClick={() => onOpen?.()}
            >
              Open the copilot
            </a>
          ) : (
            <span className="fd-copilot__open is-disabled" aria-disabled="true">
              {status.online
                ? "Online — web interface not published yet"
                : notDeployed
                  ? "Copilot not deployed"
                  : "Copilot offline"}
            </span>
          )}
        </p>
        <p className="fd-note">
          Probed over the service&apos;s own socket at {status.probedAt}
          {status.version ? ` — opencode ${status.version}` : ""}. There is no chat box on
          this page: the conversation lives in opencode&apos;s own interface, and nothing
          here imitates one.
        </p>
        <p className="fd-note">
          The copilot is <strong>operator-gated</strong> in this version. It is an agent
          that runs code, so the link above asks for a password and only the Foundry
          operators have one. This page is the public view of it: whether it is up, what
          it has spent, and what it carries.
        </p>
      </FdSection>

      <FdSection title="What it has spent">
        {usageUnavailable ? (
          <EmptyState
            variant="inline"
            title="Usage is not readable from here"
            subtitle="The token ledger lives in Postgres. Set FOUNDRY_DATABASE_URL on this deployment to read it."
          />
        ) : usage.messages === 0 ? (
          <EmptyState
            variant="inline"
            title="No copilot usage recorded yet."
            subtitle="Usage lands here when foundry:ingest-llm reads the copilot's own per-message accounting."
          />
        ) : (
          <div className="fd-copilot__stats">
            <FdStat label="Messages" value={groupDigits(usage.messages)} mono />
            <FdStat label="Sessions" value={groupDigits(usage.sessions)} mono />
            <FdStat
              label="Tokens in"
              value={groupDigits(usage.inputTokens)}
              mono
              note="measured"
            />
            <FdStat
              label="Tokens out"
              value={groupDigits(usage.outputTokens)}
              mono
              note="measured"
            />
            <FdStat label="Cost" value={usd(usage.costUsd)} mono />
          </div>
        )}
        <p className="fd-note">{pricing.label}</p>
        <p className="fd-copilot__more">
          <a href="/foundry/console/costs">The full cost ledger →</a>
        </p>
      </FdSection>

      <FdSection title="The pipeline">
        <p className="fd-copilot__pipeline">
          brief → shortGDD (pre-prod skills) → SDK7 scene (sdk-skills) → deploy to a World
          → bot-bench test
        </p>
        <p className="fd-note">
          Every skill below can be run today by naming it — <code>/pre-prod-gdd</code>,{" "}
          <code>/game-design</code>, <code>/deploy-worlds</code> — because opencode also
          publishes each one as a command, and a command is a prompt rather than a tool
          call. Two things do not work yet: the model picking a skill by itself, and
          writing the result to a file. Both need tool calls, which our model gateway does
          not emit, so the document comes back in the conversation and a person saves it.
          Nothing here pretends otherwise.
        </p>
      </FdSection>

      <FdSection
        title="Skills"
        sub={`${preProd.length} pre-production skills in the copilot workspace and ${sdk.length} SDK skills mounted from decentraland/sdk-skills.`}
      >
        <FdScrollTable ariaLabel="Installed skills">
          <table className="fd-table">
            <thead>
              <tr>
                <th scope="col">Skill</th>
                <th scope="col">What it does</th>
                <th scope="col">First commit</th>
                <th scope="col">Last commit</th>
                <th scope="col">Library</th>
              </tr>
            </thead>
            <tbody>
              {skills.map((s) => (
                <tr key={`${s.source}:${s.name}`}>
                  <th scope="row" className="fd-mono">
                    {status.online && openUrl ? (
                      <a
                        className="fd-copilot__skill"
                        href={openUrl}
                        rel="noreferrer"
                        onClick={() => onSkillLink?.(s.name)}
                      >
                        {s.name}
                      </a>
                    ) : (
                      <span className="fd-copilot__skill is-disabled">{s.name}</span>
                    )}
                  </th>
                  <td className="fd-copilot__desc">{s.description}</td>
                  <td className="fd-mono">{day(s.firstCommit)}</td>
                  <td className="fd-mono">{day(s.lastCommit)}</td>
                  <td>
                    <span className="fd-chip fd-chip--mono">{s.dir}</span>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </FdScrollTable>
        <p className="fd-note">
          Names and descriptions are each skill&apos;s own frontmatter; dates come from
          git history on the skill&apos;s directory
          {catalogSource ? `, read ${catalogSource.readAt}` : ""}. Skills that live outside
          a git checkout show no dates rather than guessed ones.
        </p>
      </FdSection>

    </div>
  );
}
