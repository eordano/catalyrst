import "./admincontrolnotice.css";

/**
 * The two honest states an admin / operator surface can be in, plus the label
 * that keeps a public read from being mistaken for a privileged one.
 *
 * These components render the *server's* answer. They do not decide anything:
 * there is no client-side authorization in this file, and adding one would be
 * theatre -- every gate behind these surfaces lives in the catalyrst crates and
 * fails closed on its own.
 *
 *   "unavailable" -- this node cannot serve the data or perform the action.
 *                   Say why, and cite the server-side check.
 *   "public"      -- the data is real but it is unauthenticated public data.
 *                   Say so, so admin chrome does not imply privilege.
 *   "sample"      -- synthetic layout data. Never a measurement.
 */
export type AdControlTone = "unavailable" | "public" | "sample";

export type AdControlNoticeProps = {
  tone?: AdControlTone;
  title: string;
  message?: string;
  /** HTTP-ish status the server answered with. 0/undefined when no call was made. */
  status?: number;
  /** file:line of the server-side authorization this surface is subject to. */
  serverCheck?: string | null;
  /** The named change that would make this available, when there is one. */
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
  /** Why this cannot be done. Rendered, not only put in a tooltip. */
  reason: string;
};

/**
 * A control that exists in the design but cannot be performed on this node.
 *
 * It is a disabled button carrying its reason -- never an enabled button that
 * would fire a request the server will refuse. The reason is both the `title`
 * (hover) and visible text, because a tooltip alone is not an explanation on
 * touch or with a screen reader.
 */
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
