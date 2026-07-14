import { Coin } from "../../atoms/icons";
import { creditsNoun } from "../credits-unit";
import "../../web/pages/marketplace.css";
import "./creditshub.css";

const STEPS = [
  { n: 1, text: "Complete your goals each week" },
  { n: 2, text: "Claim your earned Credits" },
  {
    n: 3,
    text: "Get Wearables & Emotes in the Marketplace for free! (1 Credit = $0.10)",
  },
];

export type MkCreditsGoal = {
  title: string;
  status: "progress" | "claim" | "completed" | "claimed";
  completed: number;
  total: number;
  reward: number;
};

export type MkCreditsHubData = {
  available: number;
  earned: number;
  paid: number;
  claimable: number;
  weekNumber: number;
  goals: MkCreditsGoal[];
};

type MkCreditsHubProps = {
  hub: MkCreditsHubData;
  expiresInLabel?: string;
  timeLeftLabel?: string;
  progressUnavailable?: boolean;
  onRetryProgress?: () => void;
  onClaim?: (goalTitle: string, reward: number) => void;
  onClaimAll?: () => void;
  onClose?: () => void;
};

export default function MkCreditsHub({
  hub,
  expiresInLabel = "",
  timeLeftLabel = "",
  progressUnavailable = false,
  onRetryProgress = undefined,
  onClaim = undefined,
  onClaimAll = undefined,
  onClose = undefined,
}: MkCreditsHubProps) {
  return (
    <div
      className="ep__backdrop"
      onClick={
        onClose
          ? (e) => {
              if (e.target === e.currentTarget) onClose();
            }
          : undefined
      }
    >
      <div className="mc">
        {onClose ? (
          <button
            type="button"
            className="mc__close"
            aria-label="Close"
            onClick={onClose}
          >
            ×
          </button>
        ) : null}
        <div className="mc__scroll">
          <header className="mc__head">
            <h2 className="mc__title">
              Earn Marketplace Credits,
              <br />
              Go Shopping!
            </h2>
            <p className="mc__subtitle">
              How to earn and claim your Credits
            </p>

            <div className="mc__widget" data-testid="credits-balance">
              <div className="mc__widgetlabel">Your Credits</div>
              <div className="mc__widgetval">
                <Coin size={30} className="mc__coin" />{" "}
                {progressUnavailable ? (
                  <span aria-label="Balance unavailable">—</span>
                ) : (
                  formatCredits(hub.available)
                )}
              </div>
              <div className="mc__widgetexpire" data-testid="credits-breakdown">
                {progressUnavailable
                  ? "We couldn't load your balance."
                  : hub.earned > 0 || hub.paid > 0
                    ? [
                        hub.earned > 0
                          ? `${formatCredits(hub.earned)} earned · expire in ${expiresInLabel}`
                          : null,
                        hub.paid > 0
                          ? `${formatCredits(hub.paid)} purchased · never expire`
                          : null,
                      ]
                        .filter(Boolean)
                        .join("  ·  ")
                    : "No credits expiring"}
              </div>
              <a className="mc__buycredits" href="/marketplace/packs">
                Buy Credits
              </a>
            </div>
          </header>

          <section className="mc__section">
            <div className="mc__sectionhead">
              <h3 className="mc__sectiontitle">Weekly Rewards</h3>
              <span className="mc__timeleft">
                Time Left: <b>{timeLeftLabel}</b> ·
                Week {hub.weekNumber} · Resets Sunday 11:59pm UTC
              </span>
            </div>
            <div className="mc__goallabel">Your weekly goals</div>

            {progressUnavailable ? (
              <div className="mch__empty" role="alert">
                <p style={{ margin: 0 }}>
                  We couldn't load your goals. Your progress is safe — try
                  again in a moment.
                </p>
                {onRetryProgress ? (
                  <button
                    type="button"
                    className="mc__claim"
                    style={{ marginTop: 10 }}
                    onClick={onRetryProgress}
                  >
                    Retry
                  </button>
                ) : null}
              </div>
            ) : hub.goals.length > 0 ? (
              <div className="mc__goals">
                {hub.goals.map((g) => (
                  <GoalRow key={g.title} g={g} onClaim={onClaim} />
                ))}
              </div>
            ) : (
              <p className="mch__empty">No weekly goals available right now.</p>
            )}

            {!progressUnavailable && hub.claimable > 0 && (
              <button
                type="button"
                className="mc__claimall"
                onClick={onClaimAll}
              >
                <Coin size={18} className="mc__coin" /> Claim{" "}
                {formatCredits(hub.claimable)} {creditsNoun(hub.claimable, true)}
              </button>
            )}
          </section>

          <section className="mc__section">
            <h3 className="mc__sectiontitle">How it works</h3>
            <div className="mc__steps">
              {STEPS.map((s) => (
                <div className="mc__step" key={s.n}>
                  <span className="mc__stepnum">{s.n}</span>
                  <span className="mc__steptext">{s.text}</span>
                </div>
              ))}
            </div>
          </section>
        </div>
      </div>
    </div>
  );
}

type GoalRowProps = {
  g: MkCreditsGoal;
  onClaim?: (goalTitle: string, reward: number) => void;
};

function GoalRow({ g, onClaim }: GoalRowProps) {
  const done =
    g.status === "claimed" || g.status === "completed" || g.status === "claim";
  return (
    <div className="mc__goal">
      <span className={"mc__goalcheck" + (done ? " is-done" : "")}>
        {done ? "✓" : `${g.completed}/${g.total}`}
      </span>
      <div className="mc__goalinfo">
        <div className="mc__goaltitle">{g.title}</div>
        {g.status === "progress" && (
          <div className="mc__bar">
            <span style={{ width: (g.completed / g.total) * 100 + "%" }} />
          </div>
        )}
      </div>
      <div className="mc__goalreward">
        <Coin size={15} className="mc__coin" /> +{formatCredits(g.reward)}
      </div>
      {g.status === "claim" && (
        <button
          type="button"
          className="mc__claim"
          onClick={() => onClaim?.(g.title, g.reward)}
        >
          Claim to collect
        </button>
      )}
      {g.status === "completed" && (
        <span className="mc__tag mc__tag--ok">Completed</span>
      )}
      {g.status === "claimed" && (
        <span className="mc__tag mc__tag--done">Claimed</span>
      )}
      {g.status === "progress" && <span className="mc__tag">In progress</span>}
    </div>
  );
}

function formatCredits(n: number): string {
  return Number.isInteger(n) ? String(n) : n.toFixed(2).replace(/\.?0+$/, "");
}
