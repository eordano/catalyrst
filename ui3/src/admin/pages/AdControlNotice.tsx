import "./admincontrolnotice.css";

export type AdControlTone = "unavailable" | "public" | "sample";

export type AdControlNoticeProps = {
  tone?: AdControlTone;
  title: string;
  message?: string;
  status?: number;
  serverCheck?: string | null;
  fix?: string;
};

export default function AdControlNotice({
  tone = "unavailable",
  title,
  message = undefined,
  status = undefined,
  serverCheck = undefined,
  fix = undefined,
}: AdControlNoticeProps) {
  return (
    <section
      className={`acn acn--${tone}`}
      role={tone === "unavailable" ? "alert" : "status"}
      data-tone={tone}
    >
      <h2 className="acn__title">
        {title}
        {status ? <span className="acn__status">HTTP {status}</span> : null}
      </h2>
      {message ? <p className="acn__message">{message}</p> : null}
      {fix ? <p className="acn__meta">{fix}</p> : null}
      {serverCheck ? (
        <p className="acn__meta">
          Server-side check: <code>{serverCheck}</code>
        </p>
      ) : null}
    </section>
  );
}

export type AdBlockedActionProps = {
  label: string;
  reason: string;
};

export function AdBlockedAction({ label, reason }: AdBlockedActionProps) {
  return (
    <span className="acn-blocked">
      <button type="button" disabled title={reason} className="acn-blocked__btn">
        {label}
      </button>
      <span className="acn-blocked__why">{reason}</span>
    </span>
  );
}
