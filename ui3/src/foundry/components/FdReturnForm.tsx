import { useState } from "react";

import Button from "../../atoms/Button";
import "./fdreturnform.css";

export type FdReturnFormProps = {
  pending?: boolean;
  error?: string | null;
  /** Set right after a successful redeem, while the page reloads. */
  redeemedName?: string | null;
  submitLabel?: string;
  onRedeem?: (code: string) => void;
};

export default function FdReturnForm({
  pending = false,
  error = null,
  redeemedName = null,
  submitLabel = "Return",
  onRedeem,
}: FdReturnFormProps) {
  const [code, setCode] = useState("");
  return (
    <>
      {redeemedName ? (
        <p className="fd-note" role="status">
          Welcome back, {redeemedName}. Reloading…
        </p>
      ) : (
        <form
          className="fd-return-form"
          onSubmit={(e) => {
            e.preventDefault();
            if (onRedeem && code.trim()) onRedeem(code.trim());
          }}
        >
          <input
            className="fd-form__input fd-return-form__input"
            type="text"
            value={code}
            onChange={(e) => setCode(e.target.value)}
            placeholder="xxxxx-xxxxx-xxxxx-xxxxx"
            aria-label="Return code"
            autoComplete="off"
            spellCheck={false}
          />
          <Button
            type="submit"
            variant="secondary"
            size="sm"
            disabled={pending || !onRedeem || code.trim() === ""}
          >
            {submitLabel}
          </Button>
        </form>
      )}
      {error ? (
        <p className="fd-alert" role="alert">
          {error}
        </p>
      ) : null}
    </>
  );
}
