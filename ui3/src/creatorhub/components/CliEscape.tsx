import { useCallback, useEffect, useRef, useState } from "react";
import "./cliescape.css";

export type CliEscapeProps = {
  command: string;
  explain: string;
  docs?: string;
};

export default function CliEscape({ command, explain, docs }: CliEscapeProps) {
  const [copied, setCopied] = useState(false);
  const timer = useRef<ReturnType<typeof setTimeout> | null>(null);

  useEffect(
    () => () => {
      if (timer.current) clearTimeout(timer.current);
    },
    [],
  );

  const onCopy = useCallback(() => {
    const done = () => {
      setCopied(true);
      if (timer.current) clearTimeout(timer.current);
      timer.current = setTimeout(() => setCopied(false), 2000);
    };
    const clipboard = navigator.clipboard;
    if (!clipboard) return;
    void clipboard.writeText(command).then(done, () => setCopied(false));
  }, [command]);

  return (
    <div className="cli">
      <div className="cli__bar">
        <span className="cli__hint">Run this yourself</span>
        <button type="button" className="cli__copy" onClick={onCopy}>
          {copied ? "Copied" : "Copy"}
        </button>
      </div>
      {
}
      <pre className="cli__code" tabIndex={0}>
        <code>{command}</code>
      </pre>
      <p className="cli__explain">{explain}</p>
      {docs ? (
        <p className="cli__docs">
          <a href={docs}>Docs</a>
        </p>
      ) : null}
      <span className="cli__live" role="status" aria-live="polite">
        {copied ? "Command copied to the clipboard" : ""}
      </span>
    </div>
  );
}
