import { useCallback, useEffect, useRef, useState } from "react";
import "./cliescape.css";

export type CliEscapeProps = {
  /** The exact command, verbatim. Copyable, never executable from here. */
  command: string;
  /** What the command actually does, in one sentence. */
  explain: string;
  docs?: string;
};

/**
 * The escape hatch from an unbuilt panel: the command that really works,
 * copyable.
 *
 * There is deliberately **no Run button**. The browser cannot reach
 * `dcl-one-sdk` or an explorer's `--mcp` port, so a Run control could only ever
 * report a success it did not cause -- the exact failure this whole feature
 * exists to prevent. Copy is the only affordance.
 */
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
      {/* tabIndex keeps the horizontally scrollable block reachable by keyboard;
          it is a scroll container, not a control. */}
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
