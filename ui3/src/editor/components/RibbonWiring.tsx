import { ACTION_CHIP, TRIGGER_CHIP } from "../interactions-vocab";

const label = (map: Record<string, string>, id: string | null | undefined, fallback: string): string =>
  id ? (map[id] ?? id) : fallback;

export interface RibbonWiringState {
  smart: boolean;
  wired: boolean;
  trigger?: string | null;
  action?: string | null;
}

export interface RibbonWiringProps {
  state: RibbonWiringState;
  onOpen?: () => void;
}

export default function RibbonWiring({ state, onOpen }: RibbonWiringProps) {
  if (!state.smart) {
    return <span className="rb-hint">This item has no smart-item behaviour.</span>;
  }
  return (
    <button
      type="button"
      className="rb-wiring"
      onClick={onOpen}
      disabled={onOpen === undefined}
      title={state.wired ? "Open the interactions panel" : "Nothing reacts until a trigger is added"}
    >
      <span className="rb-wire-chip trigger">{label(TRIGGER_CHIP, state.trigger, "no trigger")}</span>
      <span className="rb-wire-arrow" aria-hidden="true">
        {"\u{2192}"}
      </span>
      <span className="rb-wire-chip action">{label(ACTION_CHIP, state.action, "no action")}</span>
      <span className={"rb-wire-state" + (state.wired ? " on" : "")}>
        {state.wired ? "wired" : "not wired"}
      </span>
    </button>
  );
}
