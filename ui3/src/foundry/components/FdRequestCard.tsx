import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import "./fdrequestcard.css";

export type FdRequestCardVM = {
  id: string;
  title: string;
  body: string;
  source: string;
  status: "open" | "closed";
  pledges: number;
  pledgedByMe: boolean;
  origin: "imported" | "visitor";
};

type FdRequestCardProps = FdRequestCardVM & {
  onPledge: () => void;
  onWithdraw: () => void;
  pending?: boolean;
};

export default function FdRequestCard({
  title,
  body,
  source,
  pledges,
  status,
  pledgedByMe,
  origin,
  onPledge,
  onWithdraw,
  pending = false,
}: FdRequestCardProps) {
  const open = status === "open";

  return (
    <article className="fd-request">
      <header className="fd-request__head">
        <h3 className="fd-request__title">{title}</h3>
        <span className="fd-request__count">
          <span className="fd-request__countnum">{pledges}</span>
          <span className="fd-request__countlabel">
            {pledges === 1 ? "pledge" : "pledges"}
          </span>
        </span>
      </header>

      <p className="fd-request__text">{body}</p>

      <p className="fd-request__meta">
        <span className="fd-request__source">{source}</span>
        {origin === "imported" ? <span className="fd-chip">imported</span> : null}
        {open ? null : (
          <span className="fd-request__closed">closed to new pledges</span>
        )}
      </p>

      {open ? (
        <div className="fd-request__actions">
          {pledgedByMe ? (
            <Button variant="secondary" size="sm" onClick={onWithdraw} disabled={pending}>
              Withdraw pledge
            </Button>
          ) : (
            <Button variant="primary" size="sm" onClick={onPledge} disabled={pending}>
              Pledge to this request
            </Button>
          )}
          {pending ? <Spinner size={16} /> : null}
          {pledgedByMe ? <span className="fd-request__mine">you pledged</span> : null}
        </div>
      ) : null}
    </article>
  );
}
