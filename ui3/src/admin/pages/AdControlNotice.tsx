import Button from "../../atoms/Button";
import "../admin.css";

type AdControlTone = "unavailable" | "public" | "sample";

const TONE: Record<AdControlTone, "bad" | "info" | "warn"> = {
  unavailable: "bad",
  public: "info",
  sample: "warn",
};

type AdControlNoticeProps = {
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
      className="adm adm-notice"
      role={tone === "unavailable" ? "alert" : "status"}
      data-tone={TONE[tone]}
      data-control={tone}
    >
      <h2 className="adm-notice__title adm__h2">
        {title}
        {status ? <span className="adm-status">HTTP {status}</span> : null}
      </h2>
      {message ? <p>{message}</p> : null}
      {fix ? <p className="adm-dim">{fix}</p> : null}
      {serverCheck ? (
        <p className="adm-dim">
          Server-side check: <code>{serverCheck}</code>
        </p>
      ) : null}
    </section>
  );
}

type AdBlockedActionProps = {
  label: string;
  reason: string;
};

export function AdBlockedAction({ label, reason }: AdBlockedActionProps) {
  return (
    <div className="adm adm-card adm-card--dashed">
      <div className="adm-actions adm-actions--start">
        <Button variant="secondary" size="sm" disabled title={reason}>
          {label}
        </Button>
        <span className="adm-dim">{reason}</span>
      </div>
    </div>
  );
}
