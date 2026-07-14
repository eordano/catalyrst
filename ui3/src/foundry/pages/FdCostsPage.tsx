import EmptyState from "../../components/EmptyState";
import FdScrollTable from "../components/FdScrollTable";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
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
  byDay: readonly FdUsageDayVM[];
  recent: readonly FdUsageRowVM[];
};

export type FdPricingVM = {
  inputPerM: number;
  outputPerM: number;
  label: string;
};

/** Grouped by hand: Intl's separators move with the runtime's ICU build, and
 *  these strings have to match between the server render and hydration. */
export function groupDigits(n: number): string {
  const digits = String(Math.trunc(Math.abs(n)));
  let out = "";
  for (let i = 0; i < digits.length; i += 1) {
    if (i > 0 && (digits.length - i) % 3 === 0) out += ",";
    out += digits[i];
  }
  return n < 0 ? `-${out}` : out;
}

/** Sub-cent sums are the normal case here, so the column keeps four decimals
 *  rather than rounding a real measurement down to $0.00. */
export function usd(value: number | null): string {
  if (value === null) return "—";
  if (value === 0) return "$0.0000";
  if (value < 0.0001) return "<$0.0001";
  return `$${value.toFixed(4)}`;
}

export function utcStamp(iso: string): string {
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return iso;
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getUTCFullYear()}-${pad(d.getUTCMonth() + 1)}-${pad(
    d.getUTCDate(),
  )} ${pad(d.getUTCHours())}:${pad(d.getUTCMinutes())} UTC`;
}

export type FdCostsPageProps = {
  usage: FdUsageVM;
  pricing: FdPricingVM;
};

export default function FdCostsPage({ usage, pricing }: FdCostsPageProps) {
  const empty = usage.messages === 0;

  return (
    <div className="fd-page fd-stack fd-costs">
      <FdPageHead
        title="Copilot costs"
        intro="One row per assistant message the copilot produced, with the token counts the gateway itself reported. Nothing here is sampled or extrapolated: if a message is not in the ledger, it was not spent."
      />

      {empty ? (
        <FdSection title="Ledger">
          <EmptyState
            variant="inline"
            title="No copilot usage recorded yet."
            subtitle="Usage lands here when foundry:ingest-llm reads the copilot's own per-message accounting."
          />
        </FdSection>
      ) : (
        <>
          <FdSection title="Totals">
            <div className="fd-costs__stats">
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
              <FdStat
                label="Cost"
                value={usd(usage.costUsd)}
                mono
                note="reference pricing"
              />
            </div>
          </FdSection>

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
                      <td className="fd-mono">{d.day}</td>
                      <td className="fd-num">{groupDigits(d.inputTokens)}</td>
                      <td className="fd-num">{groupDigits(d.outputTokens)}</td>
                      <td className="fd-num">{usd(d.costUsd)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </FdScrollTable>
          </FdSection>

          <FdSection
            title="Recent messages"
            sub={
              usage.recent.length < usage.messages
                ? `Showing the ${usage.recent.length} most recent of ${groupDigits(usage.messages)} messages.`
                : undefined
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
                      <td className="fd-mono">{utcStamp(r.at)}</td>
                      <td>
                        {r.sessionTitle ?? (
                          <span className="fd-costs__dim">untitled</span>
                        )}
                      </td>
                      <td className="fd-mono">{r.model}</td>
                      <td className="fd-num">{groupDigits(r.inputTokens)}</td>
                      <td className="fd-num">{groupDigits(r.outputTokens)}</td>
                      <td className="fd-num">{usd(r.costUsd)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </FdScrollTable>
          </FdSection>
        </>
      )}

      <p className="fd-note">
        Token counts are read from the gateway&apos;s own usage accounting per message.
        Dollar figures are <strong>reference pricing</strong> — ${pricing.inputPerM.toFixed(2)} in
        / ${pricing.outputPerM.toFixed(2)} out per 1M tokens, a chosen constant for a
        self-hosted model, not a bill.
      </p>

    </div>
  );
}
