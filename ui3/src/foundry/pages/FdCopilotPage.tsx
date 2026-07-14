import Button from "../../atoms/Button";
import { groupDigits, plural, stampUTC } from "../fmt";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FD_UNAVAILABLE, FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTime from "../components/FdTime";
import { usd, type FdPricingVM, type FdUsageVM } from "./FdCostsPage";
import "./fdcopilot.css";

/** One stage of the copilot pipeline with its receipt: how many artifacts of
 *  copilot origin actually traversed it, and where those rows are listed. */
export type FdPipelineStageVM = {
  id: string;
  label: string;
  count: number;
  source: string;
  href: string | null;
};

export type FdCopilotStatusVM = {
  online: boolean;
  /** Whether the service looks provisioned at all — so "offline" can tell a
   *  failed probe of a live service apart from a service that was never deployed. */
  deployed?: boolean;
  probedAt: string;
  version?: string;
  /** Observed on the service's own session-status route: sessions stuck
   *  retrying the model gateway. null = read and clear; undefined = that route
   *  did not answer, so nothing is claimed either way. */
  gatewayStuck?: { attempts: number; message: string } | null;
};

export type FdSkillVM = {
  name: string;
  description: string;
  dir: string;
  firstCommit: string | null;
  lastCommit: string | null;
  source: "sdk-skills" | "pre-prod" | "command";
};

export type FdCatalogSourceVM = {
  readAt: string;
  note: string;
};

/** First sentence of a description; the full text rides in the sr-only twin. */
function firstSentence(text: string): string {
  const m = /^.*?[.!?](?=\s|$)/.exec(text.trim());
  return m ? m[0] : text.trim();
}

function SkillChip({ skill }: { skill: FdSkillVM }) {
  const body = (
    <>
      {skill.name}
      <span className="u-sr-only"> — {skill.description}</span>
    </>
  );
  const hint = firstSentence(skill.description);
  return skill.source === "sdk-skills" ? (
    <a
      className="fd-chip fd-chip--mono"
      href={`https://github.com/decentraland/sdk-skills/tree/main/${skill.name}`}
      rel="noreferrer"
      title={hint}
    >
      {body}
    </a>
  ) : (
    <span className="fd-chip fd-chip--mono" title={hint}>
      {body}
    </span>
  );
}

export type FdCopilotPageProps = {
  /** Whether this visitor's session already opens the copilot's door (an
   *  operator or host invite redeemed on this site). Undefined when the
   *  deployment could not check — the copy then stays neutral. */
  sessionGate?: boolean;
  status: FdCopilotStatusVM;
  usage: FdUsageVM;
  usageUnavailable?: boolean;
  /** Per-stage receipts for the pipeline; null when the database that holds
   *  them is not readable from this deployment. */
  pipeline?: readonly FdPipelineStageVM[] | null;
  /** Docs the site's own program drafted (source = 'program') — named so the
   *  copilot-only receipts above cannot be read as counting them. Null when the
   *  database is not readable from this deployment. */
  programDrafted?: readonly { id: string; title: string }[] | null;
  skills: readonly FdSkillVM[];
  catalogSource?: FdCatalogSourceVM;
  /** null when the web interface has no published URL yet (backend can still be
   *  up on its socket — the CTA then reads as unavailable rather than linking
   *  somewhere that does not answer). */
  openUrl: string | null;
  pricing: FdPricingVM;
  /** Door funnel over the last week — sessions the door minted vs sessions
   *  that got at least one model reply. Null when either backend was mute. */
  door?: { minted: number; replied: number; lastMintedAt: string | null } | null;
  /** Last verdict of the hourly cold-profile door walk; null when the probe
   *  is not configured or has not run. */
  doorProbe?: {
    ok: boolean;
    at: string;
    steps: readonly { name: string; ok: boolean; detail: string }[];
  } | null;
  onOpen?: () => void;
};

export default function FdCopilotPage({
  sessionGate,
  status,
  usage,
  usageUnavailable = false,
  pipeline = null,
  programDrafted = null,
  skills,
  catalogSource,
  door = null,
  doorProbe = null,
  openUrl,
  pricing,
  onOpen,
}: FdCopilotPageProps) {
  const sdk = skills.filter((s) => s.source === "sdk-skills");
  const preProd = skills.filter((s) => s.source === "pre-prod");
  const commands = skills.filter((s) => s.source === "command");
  const notDeployed = !status.online && status.deployed === false;
  const stuck = status.online ? status.gatewayStuck : null;

  const state = status.online ? (stuck ? "stuck" : "online") : notDeployed ? "absent" : "offline";
  const pillText = {
    online: "Online",
    stuck: "Model not answering",
    offline: "Offline",
    absent: "Not deployed",
  }[state];
  const pillTitle = {
    online: `probed ${stampUTC(status.probedAt)}`,
    stuck: `the model gateway refused ${stuck?.attempts ?? 0} attempts in a row`,
    offline: "deployed, but did not answer the probe",
    absent: "no copilot service is provisioned on this server",
  }[state];
  const pillClass = { online: "is-online", stuck: "is-stuck", offline: "is-offline", absent: "is-offline" }[state];
  const ctaTitle = status.online
    ? "the web interface has no published URL yet"
    : pillTitle;

  const probeMessages = usage.probeMessages ?? 0;
  const probeTokens = usage.probeTokens ?? 0;
  const probeCostUsd = usage.probeCostUsd ?? 0;
  const draftMessages = Math.max(0, usage.messages - probeMessages);
  const draftTokens = Math.max(
    0,
    usage.inputTokens + usage.outputTokens - probeTokens,
  );
  const draftCostUsd = Math.max(0, usage.costUsd - probeCostUsd);

  return (
    <div className="fd-page fd-stack fd-copilot">
      <FdPageHead
        title="The copilot"
        intro="A self-hosted opencode instance wired to our own model gateway."
        aside={
          <>
            <span className={"fd-chip fd-copilot__pill " + pillClass} title={pillTitle}>
              {pillText}
            </span>
            {status.online && openUrl ? (
              <Button
                as="a"
                variant="primary"
                size="sm"
                href={openUrl}
                rel="noreferrer"
                title={
                  sessionGate
                    ? "your session opens the door"
                    : sessionGate === false
                      ? "your session holds no host or operator role — redeem an invite on the People page first"
                      : undefined
                }
                onClick={() => onOpen?.()}
              >
                Open the copilot
              </Button>
            ) : (
              <Button size="sm" disabled title={ctaTitle}>
                Open the copilot
              </Button>
            )}
          </>
        }
      />

      <FdSection title="Status">
        {stuck ? (
          <p className="fd-copilot__gateway">
            Its model gateway has refused {stuck.attempts} connection attempts in a
            row, so conversations get no reply.
            {stuck.message ? (
              <>
                {" "}
                It reports <code>{stuck.message}</code>.
              </>
            ) : null}
          </p>
        ) : null}
        <dl className="fd-facts">
          <div>
            <dt>Probed</dt>
            <dd>
              <FdTime iso={status.probedAt} title="over the service's own socket">
                {stampUTC(status.probedAt)}
              </FdTime>
            </dd>
          </div>
          {status.version ? (
            <div>
              <dt>Version</dt>
              <dd className="fd-mono">opencode {status.version}</dd>
            </div>
          ) : null}
        </dl>
        <p className="fd-note">
          Its door opens for sessions holding an operator or host invite from{" "}
          <a href="/foundry/people">People</a>; the operator password remains
          the fallback.
        </p>
      </FdSection>

      <FdSection title="The door, measured">
        {doorProbe ? (
          <p className="fd-copilot__doorprobe">
            <span
              className={
                "fd-chip fd-copilot__pill " + (doorProbe.ok ? "is-online" : "is-stuck")
              }
              title="an hourly cold browser profile recovers the probe persona, walks the door, and checks the composer"
            >
              {doorProbe.ok ? "Cold walk: pass" : "Cold walk: fail"}
            </span>{" "}
            <FdTime iso={doorProbe.at} title="when the walk ran">{stampUTC(doorProbe.at)}</FdTime>
            {" — "}
            {doorProbe.ok
              ? `${doorProbe.steps.length} steps: ${doorProbe.steps.map((s) => s.name).join(", ")}`
              : `failed at ${doorProbe.steps.find((s) => !s.ok)?.name ?? "unknown"}: ${doorProbe.steps.find((s) => !s.ok)?.detail ?? ""}`}
          </p>
        ) : (
          <p className="fd-note">
            The hourly cold-profile walk has not reported here — its verdict file
            is absent or unreadable from this deployment.
          </p>
        )}
        {door ? (
          <dl className="fd-facts">
            <div>
              <dt>Minted</dt>
              <dd title="sessions the door created in the last 7 days">{door.minted}</dd>
            </div>
            <div>
              <dt>Answered</dt>
              <dd title="of those, sessions with at least one model reply on the ledger">
                {door.replied}
              </dd>
            </div>
            <div>
              <dt>Stranded</dt>
              <dd title="door sessions that never got a reply — the number this page exists to keep at zero">
                {door.minted - door.replied}
              </dd>
            </div>
            {door.lastMintedAt ? (
              <div>
                <dt>Last entry</dt>
                <dd>
                  <FdTime iso={door.lastMintedAt} title="newest door session">{stampUTC(door.lastMintedAt)}</FdTime>
                </dd>
              </div>
            ) : null}
          </dl>
        ) : (
          <p className="fd-note">
            Door counts could not be read — the session list or the usage ledger
            did not answer.
          </p>
        )}
      </FdSection>

      <FdSection
        title="What it has spent"
        aside={
          <Button as="a" variant="secondary" size="sm" href="/foundry/console/costs">
            The cost ledger
          </Button>
        }
      >
        {usageUnavailable ? (
          <p className="fd-empty">{FD_UNAVAILABLE}</p>
        ) : usage.messages === 0 ? (
          <p className="fd-empty">No copilot usage recorded yet.</p>
        ) : (
          <>
            <div className="fd-statrow">
              <FdStat
                label="Messages"
                value={groupDigits(draftMessages)}
                mono
                title="copilot drafting; the deploy probe is counted apart"
              />
              <FdStat
                label="Tokens"
                value={groupDigits(draftTokens)}
                mono
                note="measured"
                title="input plus output on those messages"
              />
              <FdStat
                label="Cost"
                value={usd(draftCostUsd)}
                mono
                note="drafting, reference pricing"
                title={pricing.label}
              />
            </div>
            {probeMessages > 0 ? (
              <div className="fd-chiprow fd-copilot__probe">
                <span
                  className="fd-chip"
                  title="the deploy pipeline's own gateway check"
                >
                  Deploy probe · {plural(probeMessages, "message")} ·{" "}
                  {groupDigits(probeTokens)} tokens · {usd(probeCostUsd)}
                </span>
              </div>
            ) : null}
          </>
        )}
      </FdSection>

      <FdSection title="The pipeline">
        <p className="fd-copilot__pipeline">
          brief → <a href="/foundry/gdd">shortGDD</a> → SDK7 scene →{" "}
          <a href="/foundry/play">deploy to a World</a> →{" "}
          <a href="/foundry/console/bench">bot-bench test</a>
        </p>
        {pipeline ? (
          <FdScrollTable ariaLabel="Pipeline receipts">
            <table className="fd-table fd-copilot__receipts">
              <thead>
                <tr>
                  <th scope="col">Stage</th>
                  <th scope="col">Copilot artifacts</th>
                </tr>
              </thead>
              <tbody>
                {pipeline.map((stage) => (
                  <tr key={stage.id}>
                    <th scope="row">{stage.label}</th>
                    <td className="fd-table__num" title={stage.source}>
                      {stage.href ? (
                        <a href={stage.href}>{stage.count}</a>
                      ) : (
                        stage.count
                      )}
                      <span className="u-sr-only"> — {stage.source}</span>
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </FdScrollTable>
        ) : (
          <p className="fd-empty">{FD_UNAVAILABLE}</p>
        )}
        {programDrafted && programDrafted.length > 0 ? (
          <p className="fd-note">
            {programDrafted.length === 1 && programDrafted[0] ? (
              <>
                These count copilot-drafted docs only.{" "}
                <a href={`/foundry/gdd/${programDrafted[0].id}`}>
                  {programDrafted[0].title}
                </a>{" "}
                was drafted by the site&apos;s own program instead.
              </>
            ) : (
              <>
                These count copilot-drafted docs only. {programDrafted.length}{" "}
                docs on <a href="/foundry/gdd">the shelf</a> were drafted by the
                site&apos;s own program instead.
              </>
            )}
          </p>
        ) : null}
      </FdSection>

      <FdSection
        title="Skills & commands"
        badge={
          <span
            className="fd-chip"
            title={
              catalogSource
                ? `skill frontmatter and git history, read ${catalogSource.readAt}`
                : undefined
            }
          >
            {skills.length}
          </span>
        }
      >
        {skills.length === 0 ? (
          <p className="fd-empty">No skills installed.</p>
        ) : (
          <div className="fd-copilot__skills">
            {commands.length > 0 ? (
              <div className="fd-copilot__skillgroup">
                <h3 className="fd-subhead">Commands</h3>
                <p className="fd-note">
                  Typed in the chat — each expands into its full brief before
                  the model answers.
                </p>
                <div className="fd-chiprow">
                  {commands.map((s) => (
                    <SkillChip key={`command:${s.name}`} skill={s} />
                  ))}
                </div>
              </div>
            ) : null}
            {preProd.length > 0 ? (
              <div className="fd-copilot__skillgroup">
                <h3 className="fd-subhead">Copilot workspace</h3>
                <div className="fd-chiprow">
                  {preProd.map((s) => (
                    <SkillChip key={`pre-prod:${s.name}`} skill={s} />
                  ))}
                </div>
              </div>
            ) : null}
            {sdk.length > 0 ? (
              <div className="fd-copilot__skillgroup">
                <h3 className="fd-subhead">decentraland/sdk-skills</h3>
                <div className="fd-chiprow">
                  {sdk.map((s) => (
                    <SkillChip key={`sdk:${s.name}`} skill={s} />
                  ))}
                </div>
              </div>
            ) : null}
          </div>
        )}
      </FdSection>
    </div>
  );
}
