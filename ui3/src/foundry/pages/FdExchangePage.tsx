import { useState, type FormEvent } from "react";
import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdRequestCard, {
  type FdRequestCardVM,
} from "../components/FdRequestCard";
import FdSection, { FdPageHead } from "../components/FdSection";
import { plural } from "../fmt";

export type FdExchangeFormErrors = Partial<
  Record<"title" | "body" | "source", string>
>;

export const FD_BODY_MAX = 280;

export type FdExchangePageProps = {
  requests: readonly FdRequestCardVM[];
  postOpen: boolean;
  /** Seeds the post form title (a doc arriving via "Take this design on") —
   *  visible, editable text, never auto-submitted. */
  draftTitle?: string | null;
  onTogglePost: () => void;
  formErrors?: FdExchangeFormErrors;
  error?: string | null;
  pending?: boolean;
  viewerIsAdmin?: boolean;
  onPledge: (requestId: string) => void;
  onWithdraw: (requestId: string) => void;
  onModerate?: (requestId: string, verdict: "approved" | "closed") => void;
  onPost: (values: { title: string; body: string; source: string }) => void;
};

// Its own component so the draft dies with the form: closing it (or a
// successful post, which closes it) unmounts and clears, while a failed
// submit keeps the form mounted and the text intact.
function FdPostRequestForm({
  draftTitle,
  formErrors,
  error,
  pending,
  onPost,
}: Pick<
  FdExchangePageProps,
  "draftTitle" | "formErrors" | "error" | "pending" | "onPost"
>) {
  const [body, setBody] = useState("");
  const [overLimit, setOverLimit] = useState(false);

  function submitPost(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    const bodyText = String(form.get("body") ?? "");
    if (bodyText.length > FD_BODY_MAX) {
      setOverLimit(true);
      return;
    }
    setOverLimit(false);
    onPost({
      title: String(form.get("title") ?? ""),
      body: bodyText,
      source: String(form.get("source") ?? ""),
    });
  }

  const bodyError = overLimit
    ? `Keep the description under ${FD_BODY_MAX} characters — it is ${body.length} now. Nothing was posted; your text is kept.`
    : formErrors?.body ?? null;
  // The create failure repeats next to the submit button, where the eye is
  // after a click — the field-level line alone sat below the fold on phones.
  const actionAlert =
    error ?? bodyError ?? formErrors?.title ?? formErrors?.source ?? null;

  return (
    <form className="fd-form" method="post" onSubmit={submitPost}>
      <h3 className="fd-form__title">Post what you wish existed</h3>

      <div className="fd-form__field">
        <label className="fd-form__label" htmlFor="fd-req-title">
          Title
        </label>
        <input
          id="fd-req-title"
          className="fd-form__input"
          name="title"
          type="text"
          maxLength={80}
          required
          autoComplete="off"
          defaultValue={draftTitle ?? undefined}
        />
        {formErrors?.title ? (
          <p className="fd-alert" role="alert">
            {formErrors.title}
          </p>
        ) : null}
      </div>

      <div className="fd-form__field">
        <div className="fd-form__labelrow">
          <label className="fd-form__label" htmlFor="fd-req-body">
            What is missing, and what would you show up for
          </label>
          <span
            id="fd-req-body-count"
            className={
              "fd-form__count" + (body.length > FD_BODY_MAX ? " is-over" : "")
            }
          >
            {body.length} / {FD_BODY_MAX}
          </span>
        </div>
        {/* No maxLength: a silently truncated paste loses words with no
            trace. The counter and the submit check carry the limit. */}
        <textarea
          id="fd-req-body"
          className="fd-form__textarea"
          name="body"
          rows={3}
          required
          value={body}
          onChange={(e) => {
            setBody(e.target.value);
            if (overLimit && e.target.value.length <= FD_BODY_MAX)
              setOverLimit(false);
          }}
          aria-describedby="fd-req-body-count"
        />
        {bodyError ? (
          <p className="fd-alert" role="alert">
            {bodyError}
          </p>
        ) : null}
      </div>

      <div className="fd-form__field">
        <label className="fd-form__label" htmlFor="fd-req-source">
          Where the ask came from (optional)
        </label>
        {/* Optional on purpose: forcing this field made self-originated asks
            invent a community. Left blank, the ask is recorded first-person —
            "my own ask, made here". */}
        <input
          id="fd-req-source"
          className="fd-form__input"
          name="source"
          type="text"
          maxLength={60}
          autoComplete="off"
          placeholder="e.g. Discord #creators — or leave blank: my own ask"
        />
        {formErrors?.source ? (
          <p className="fd-alert" role="alert">
            {formErrors.source}
          </p>
        ) : null}
      </div>

      <div className="fd-form__actions">
        <Button type="submit" variant="primary" size="sm" disabled={pending}>
          Post the request
        </Button>
        {pending ? <Spinner size={16} /> : null}
        {actionAlert ? (
          <span className="fd-alert fd-form__alert" role="alert">
            {actionAlert}
          </span>
        ) : (
          <span className="fd-note-inline">
            Everyone who visits sees it and can pledge.
          </span>
        )}
      </div>
    </form>
  );
}

export default function FdExchangePage({
  requests,
  postOpen,
  draftTitle = null,
  onTogglePost,
  formErrors,
  error = null,
  pending = false,
  viewerIsAdmin = false,
  onPledge,
  onWithdraw,
  onModerate,
  onPost,
}: FdExchangePageProps) {
  return (
    <div className="fd-page fd-stack fd-exchange">
      <FdPageHead
        title="Exchange"
        aside={
          <Button variant="secondary" size="sm" onClick={onTogglePost}>
            {postOpen ? "Close the form" : "Post a request"}
          </Button>
        }
      />

      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}

      {postOpen ? (
        <FdPostRequestForm
          draftTitle={draftTitle}
          formErrors={formErrors}
          error={error}
          pending={pending}
          onPost={onPost}
        />
      ) : null}

      <FdSection
        title="Requests"
        badge={
          requests.length > 0 ? (
            <span className="fd-chip">{plural(requests.length, "request")}</span>
          ) : undefined
        }
      >
        {requests.length === 0 ? (
          <p className="fd-empty">No requests yet.</p>
        ) : (
          <div className="fd-board">
            {requests.map((request) => (
              <FdRequestCard
                key={request.id}
                {...request}
                pending={pending}
                viewerIsAdmin={viewerIsAdmin}
                onPledge={() => onPledge(request.id)}
                onWithdraw={() => onWithdraw(request.id)}
                onModerate={
                  onModerate
                    ? (verdict) => onModerate(request.id, verdict)
                    : undefined
                }
              />
            ))}
          </div>
        )}
      </FdSection>
    </div>
  );
}
