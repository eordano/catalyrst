import { groupDigits, plural, stampUTC } from "../fmt";
import Button from "../../atoms/Button";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FD_UNAVAILABLE, FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import FdTime from "../components/FdTime";
import "../components/fdstat.css";
import "./fdcosts.css";

export type FdUsageRowVM = {
  messageId: string;
  sessionTitle: string | null;
  model: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number | null;
  at: string;
};

export type FdUsageDayVM = {
  day: string;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
};

export type FdUsageVM = {
  messages: number;
  sessions: number;
  inputTokens: number;
  outputTokens: number;
  costUsd: number;
  /** Messages/tokens in the deploy's own verification session — counted in the
   *  totals, named here so the headline can say how much of itself is the
   *  deploy proving the gateway answers rather than copilot drafting. */
  probeMessages?: number;
  probeTokens?: number;
  probeCostUsd?: number;
  byDay: readonly FdUsageDayVM[];
  recent: readonly FdUsageRowVM[];
};

/** The session title the deploy's own gateway probe records its usage under.
 *  Kept in lockstep with DEPLOY_PROBE_SESSION_TITLE in the data layer. */
export const DEPLOY_PROBE_SESSION_TITLE = "deploy-proof";

export type FdPricingVM = {
  inputPerM: number;
  outputPerM: number;
  label: string;
};

export { groupDigits };

/** Sub-cent sums are the normal case here, so the column keeps four decimals
 *  rather than rounding a real measurement down to $0.00. */
export function usd(value: number | null): string {
  if (value === null) return "—";
  if (value === 0) return "$0.0000";
  if (value < 0.0001) return "<$0.0001";
  return `$${value.toFixed(4)}`;
}

export type FdCostsPageProps = {
  usage: FdUsageVM;
  pricing: FdPricingVM;
  /** The ledger could not be read on this deployment. */
  unavailable?: boolean;
};

export default function FdCostsPage({
  usage,
  pricing,
  unavailable = false,
}: FdCostsPageProps) {
  const empty = usage.messages === 0;
  const probeMessages = usage.probeMessages ?? 0;
  const pricingNote = `reference pricing: $${pricing.inputPerM.toFixed(2)} in / $${pricing.outputPerM.toFixed(2)} out per 1M tokens, a chosen constant for a self-hosted model`;

  if (unavailable) {
    return (
      <div className="fd-stack fd-costs">
        <FdPageHead title="Copilot costs" />
        <p className="fd-empty">{FD_UNAVAILABLE}</p>
      </div>
    );
  }

  return (
    <div className="fd-stack fd-costs">
      <FdPageHead
        title="Copilot costs"
        aside={
          <Button as="a" variant="secondary" size="sm" href="/foundry/copilot">
            The copilot
          </Button>
        }
      />

      {empty ? (
        <FdSection title="Recent messages">
          <p className="fd-empty">No copilot usage recorded yet.</p>
        </FdSection>
      ) : (
        <>
          <FdSection title="Totals">
            <div className="fd-statrow">
              <FdStat
                label="Messages"
                value={groupDigits(usage.messages)}
                mono
                title="one row per assistant message the gateway metered"
              />
              <FdStat
                label="Sessions"
                value={groupDigits(usage.sessions)}
                mono
                title="distinct copilot sessions in the ledger"
              />
              <FdStat
                label="Tokens in"
                value={groupDigits(usage.inputTokens)}
                mono
                title="measured by the gateway, per message"
              />
              <FdStat
                label="Tokens out"
                value={groupDigits(usage.outputTokens)}
                mono
                title="measured by the gateway, per message"
              />
              <FdStat
                label="Cost"
                value={usd(usage.costUsd)}
                mono
                note="reference pricing"
                title={pricingNote}
              />
              {probeMessages > 0 ? (
                <FdStat
                  label="Deploy probe"
                  value={groupDigits(probeMessages)}
                  mono
                  note={`of those messages · ${groupDigits(usage.probeTokens ?? 0)} tokens`}
                  title={`the session titled ${DEPLOY_PROBE_SESSION_TITLE} — the deploy proving the gateway answers, not copilot drafting`}
                />
              ) : null}
            </div>
          </FdSection>

          {usage.byDay.length > 1 ? (
            <FdSection title="By day">
              <FdScrollTable ariaLabel="Copilot usage by day">
                <table className="fd-table">
                  <thead>
                    <tr>
                      <th scope="col">Day (UTC)</th>
                      <th scope="col">Tokens in</th>
                      <th scope="col">Tokens out</th>
                      <th scope="col">Cost</th>
                    </tr>
                  </thead>
                  <tbody>
                    {usage.byDay.map((d) => (
                      <tr key={d.day}>
                        <td className="fd-table__mono">
                          <FdTime iso={d.day}>{d.day}</FdTime>
                        </td>
                        <td className="fd-table__num">{groupDigits(d.inputTokens)}</td>
                        <td className="fd-table__num">{groupDigits(d.outputTokens)}</td>
                        <td className="fd-table__num" title={pricingNote}>
                          {usd(d.costUsd)}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </FdScrollTable>
            </FdSection>
          ) : null}

          <FdSection
            title="Recent messages"
            badge={
              <span className="fd-chip">{plural(usage.recent.length, "message")}</span>
            }
          >
            <FdScrollTable ariaLabel="Recent copilot messages">
              <table className="fd-table">
                <thead>
                  <tr>
                    <th scope="col">When</th>
                    <th scope="col">Session</th>
                    <th scope="col">Model</th>
                    <th scope="col">In</th>
                    <th scope="col">Out</th>
                    <th scope="col">Cost</th>
                  </tr>
                </thead>
                <tbody>
                  {usage.recent.map((r) => (
                    <tr key={r.messageId}>
                      <td className="fd-table__mono">
                        <FdTime iso={r.at} title={r.at}>
                          {stampUTC(r.at)}
                        </FdTime>
                      </td>
                      <td>
                        {r.sessionTitle ?? (
                          <span className="fd-costs__dim">untitled</span>
                        )}
                        {r.sessionTitle === DEPLOY_PROBE_SESSION_TITLE ? (
                          <>
                            {" "}
                            <span
                              className="fd-chip"
                              title="the deploy proving the gateway answers, not copilot drafting"
                            >
                              deploy probe
                            </span>
                          </>
                        ) : null}
                      </td>
                      <td className="fd-table__mono">{r.model}</td>
                      <td className="fd-table__num">{groupDigits(r.inputTokens)}</td>
                      <td className="fd-table__num">{groupDigits(r.outputTokens)}</td>
                      <td className="fd-table__num" title={pricingNote}>
                        {usd(r.costUsd)}
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </FdScrollTable>
            <p className="fd-note">
              Transcripts are not stored — this ledger holds only the per-message
              token accounting the gateway reported.
            </p>
          </FdSection>
        </>
      )}
    </div>
  );
}
