import type { FormEvent } from "react";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import EmptyState from "../../components/EmptyState";
import FdRequestCard, {
  type FdRequestCardVM,
} from "../components/FdRequestCard";
import FdSection, { FdPageHead } from "../components/FdSection";
import FdStat from "../components/FdStat";
import "../components/fdstat.css";
import "./fdexchange.css";

export type FdExchangeStats = {
  openRequests: number;
  totalPledges: number;
};

export type FdExchangeFormErrors = Partial<
  Record<"title" | "body" | "source", string>
>;

export type FdExchangePageProps = {
  stats: FdExchangeStats;
  requests: readonly FdRequestCardVM[];
  postOpen: boolean;
  onTogglePost: () => void;
  formErrors?: FdExchangeFormErrors;
  error?: string | null;
  pending?: boolean;
  onPledge: (requestId: string) => void;
  onWithdraw: (requestId: string) => void;
  onPost: (values: { title: string; body: string; source: string }) => void;
};

export default function FdExchangePage({
  stats,
  requests,
  postOpen,
  onTogglePost,
  formErrors,
  error = null,
  pending = false,
  onPledge,
  onWithdraw,
  onPost,
}: FdExchangePageProps) {
  const open = requests.filter((r) => r.status === "open");
  const top = open.reduce<FdRequestCardVM | null>(
    (best, r) => (best === null || r.pledges > best.pledges ? r : best),
    null,
  );

  function submitPost(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    onPost({
      title: String(form.get("title") ?? ""),
      body: String(form.get("body") ?? ""),
      source: String(form.get("source") ?? ""),
    });
  }

  return (
    <div className="fd-page fd-stack fd-exchange">
      <FdPageHead
        title="Exchange"
        intro="Ask for what you want built, and pledge on the requests other people posted. Both numbers below are row counts on this site — nothing else feeds them."
        aside={
          <Button variant="secondary" size="sm" onClick={onTogglePost}>
            {postOpen ? "Close the form" : "Post a request"}
          </Button>
        }
      />

      <div className="fd-statrow">
        <FdStat
          label="Open requests"
          value={stats.openRequests}
          note="Posted by visitors; one row each."
        />
        <FdStat
          label="Pledges on the board"
          value={stats.totalPledges}
          note="One row per session per request. No pledge is counted twice."
        />
        <FdStat
          label="Most-pledged request"
          value={top ? top.pledges : 0}
          note={
            top
              ? "The highest pledge count on an open request right now."
              : "Nothing is open yet."
          }
        />
      </div>

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      {postOpen ? (
        <form className="fd-exchange__form" method="post" onSubmit={submitPost}>
          <h2 className="fd-exchange__formtitle">Post what you wish existed</h2>

          <label className="fd-exchange__label" htmlFor="fd-req-title">
            Title
          </label>
          <input
            id="fd-req-title"
            className="fd-exchange__input"
            name="title"
            type="text"
            maxLength={80}
            required
            autoComplete="off"
          />
          {formErrors?.title ? (
            <p className="fd-alert" role="alert">
              {formErrors.title}
            </p>
          ) : null}

          <label className="fd-exchange__label" htmlFor="fd-req-body">
            What is missing, and what would you show up for
          </label>
          <textarea
            id="fd-req-body"
            className="fd-exchange__input fd-exchange__textarea"
            name="body"
            maxLength={280}
            rows={3}
            required
          />
          {formErrors?.body ? (
            <p className="fd-alert" role="alert">
              {formErrors.body}
            </p>
          ) : null}

          <label className="fd-exchange__label" htmlFor="fd-req-source">
            Where the ask came from
          </label>
          <input
            id="fd-req-source"
            className="fd-exchange__input"
            name="source"
            type="text"
            maxLength={60}
            required
            autoComplete="off"
          />
          {formErrors?.source ? (
            <p className="fd-alert" role="alert">
              {formErrors.source}
            </p>
          ) : null}

          <div className="fd-exchange__formactions">
            <Button type="submit" variant="primary" size="sm" disabled={pending}>
              Post the request
            </Button>
            {pending ? <Spinner size={16} /> : null}
            <span className="fd-note">
              Your request is shared state: every other visitor sees it and can pledge.
            </span>
          </div>
        </form>
      ) : null}

      <FdSection
        title="Requests"
        sub="Pledges are the demand signal. Nothing here is approved or scheduled by this site — a request is a public ask with a count attached."
      >
        {requests.length === 0 ? (
          <EmptyState
            variant="inline"
            title="No requests yet"
            subtitle="Ask for what you want built — the board starts empty on purpose."
            actions={[{ label: "Post the first request", onClick: onTogglePost }]}
          />
        ) : (
          <div className="fd-exchange__board">
            {requests.map((request) => (
              <FdRequestCard
                key={request.id}
                {...request}
                pending={pending}
                onPledge={() => onPledge(request.id)}
                onWithdraw={() => onWithdraw(request.id)}
              />
            ))}
          </div>
        )}
      </FdSection>

    </div>
  );
}
