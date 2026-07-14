import { ACTION_CHIP, TRIGGER_CHIP } from "../interactions-vocab";

const label = (map: Record<string, string>, id: string | null | undefined, fallback: string): string =>
  id ? (map[id] ?? id) : fallback;

export interface RibbonWiringState {
  /** The entity carries at least one asset-packs component. */
  smart: boolean;
  /** It has at least one trigger, which is the only thing that makes it react. */
  wired: boolean;
  trigger?: string | null;
  action?: string | null;
}

export interface RibbonWiringProps {
  state: RibbonWiringState;
  onOpen?: () => void;
}

// Read-only on purpose: the prototype let these chips cycle through the trigger
// vocabulary without ever writing a component, so an item could read
// "on_delay -> teleport_player" and still be unwired.
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
