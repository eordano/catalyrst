import { useEffect, useState } from "react";
import "./toggle.css";

type ToggleProps = {
  checked?: boolean;
  defaultChecked?: boolean;
  onChange?: (checked: boolean) => void;
  ariaLabel?: string;
  ariaLabelledBy?: string;
  disabled?: boolean;
};

export default function Toggle({ checked, defaultChecked = false, onChange, ariaLabel, ariaLabelledBy, disabled = false }: ToggleProps) {
  const [internal, setInternal] = useState(defaultChecked);
  const isControlled = checked !== undefined;
  const on = isControlled ? checked : internal;

  useEffect(() => {
    if (import.meta.env.DEV && !ariaLabel && !ariaLabelledBy) {
      console.warn(
        "Toggle: rendered without ariaLabel/ariaLabelledBy \u{2014} the switch has no accessible name. Pass ariaLabel (or ariaLabelledBy) so screen readers can announce what it controls."
      );
    }
  }, [ariaLabel, ariaLabelledBy]);

  function toggle() {
    if (!isControlled) setInternal(!on);
    onChange?.(!on);
  }

  return (
    <button
      type="button"
      role="switch"
      aria-checked={on}
      aria-label={ariaLabel}
      aria-labelledby={ariaLabelledBy}
      className={"toggle" + (on ? " is-on" : "")}
      disabled={disabled}
      onClick={toggle}
    >
      <span className="toggle__knob" />
    </button>
  );
}
