import { type FormEvent } from "react";

import Button from "../../atoms/Button";
import Spinner from "../../atoms/Spinner";
import FdSection, { FdPageHead } from "../components/FdSection";
import "./fdregister.css";

export type FdRegisterGamePageProps = {
  /** Presentation gate only — the server re-checks the host role in-tx. */
  canHost: boolean;
  error?: string | null;
  /** Slug of the just-registered game; the success line links its new page. */
  registeredSlug?: string | null;
  pending?: boolean;
  onRegister: (values: {
    id: string;
    title: string;
    repoPath: string;
    gddDocId: string;
    sourceNote: string;
  }) => void;
};

export default function FdRegisterGamePage({
  canHost,
  error = null,
  registeredSlug = null,
  pending = false,
  onRegister,
}: FdRegisterGamePageProps) {
  function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const form = new FormData(event.currentTarget);
    onRegister({
      id: String(form.get("id") ?? ""),
      title: String(form.get("title") ?? ""),
      repoPath: String(form.get("repoPath") ?? ""),
      gddDocId: String(form.get("gddDocId") ?? ""),
      sourceNote: String(form.get("sourceNote") ?? ""),
    });
  }

  return (
    <div className="fd-page fd-stack">
      <FdPageHead
        eyebrow="Games"
        title="Register a game"
        crumbs={<a href="/foundry/play">← All games</a>}
      />

      {!canHost ? (
        <FdSection title="This needs a host">
          <p className="fd-note">
            Registering a game on the shelf needs the host role. Redeem a host
            invite on <a href="/foundry/people">People</a>, or ask a host on
            the roster there to register it for you.
          </p>
        </FdSection>
      ) : (
        <FdSection
          title="The row you are writing"
          sub="A registered game is a repo row: the shelf shows exactly what you enter here. Deployment facts — world, size, dates — are read only from the worlds mirror, so they stay empty until an import reads them."
        >
          {registeredSlug ? (
            <p className="fd-note" role="status">
              Registered. The game now has its own page —{" "}
              <a href={`/foundry/play/${encodeURIComponent(registeredSlug)}`}>
                /foundry/play/{registeredSlug}
              </a>
              — and the registration is on the timeline.
            </p>
          ) : null}
          {error ? (
            <p className="fd-alert" role="alert">
              {error}
            </p>
          ) : null}
          <form className="fd-form fd-register" method="post" onSubmit={submit}>
            <input type="hidden" name="intent" value="register" />
            <div className="fd-form__row">
              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-register-id">
                  Id
                </label>
                <input
                  id="fd-register-id"
                  className="fd-form__input"
                  name="id"
                  type="text"
                  autoComplete="off"
                  placeholder="lowercase-with-dashes"
                  required
                />
              </div>
              <div className="fd-form__field">
                <label className="fd-form__label" htmlFor="fd-register-title">
                  Title
                </label>
                <input
                  id="fd-register-title"
                  className="fd-form__input"
                  name="title"
                  type="text"
                  autoComplete="off"
                  required
                />
              </div>
            </div>
            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-register-source">
                Source note
              </label>
              <input
                id="fd-register-source"
                className="fd-form__input"
                name="sourceNote"
                type="text"
                autoComplete="off"
                placeholder="where this game comes from — shown on its card"
                required
              />
            </div>
            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-register-repo">
                Repo path (optional)
              </label>
              <input
                id="fd-register-repo"
                className="fd-form__input"
                name="repoPath"
                type="text"
                autoComplete="off"
              />
            </div>
            <div className="fd-form__field">
              <label className="fd-form__label" htmlFor="fd-register-doc">
                Design doc id (optional)
              </label>
              <input
                id="fd-register-doc"
                className="fd-form__input"
                name="gddDocId"
                type="text"
                autoComplete="off"
                placeholder="an existing id from /foundry/gdd"
              />
            </div>
            <div className="fd-form__actions">
              <Button
                type="submit"
                variant="primary"
                size="sm"
                disabled={pending}
              >
                Register
              </Button>
              {pending ? <Spinner size={16} /> : null}
            </div>
          </form>
        </FdSection>
      )}
    </div>
  );
}
