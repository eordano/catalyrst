
type ContractProp = { kind: string; values?: (string | number)[]; optional?: boolean };
type ContractEvent = { loose: boolean; props: Record<string, ContractProp> };
export type TelemetryContract = { events: Record<string, ContractEvent> };

function kindOf(v: unknown): string {
  if (typeof v === "string") return "string";
  if (typeof v === "number") return "number";
  if (typeof v === "boolean") return "boolean";
  return v === null ? "null" : typeof v;
}

export function validateEventAgainst(
  contract: TelemetryContract,
  event: string,
  props: Record<string, unknown> | null | undefined,
): string[] {
  const ev = contract.events[event];
  if (!ev) return [`unknown event "${event}" (not in the telemetry contract)`];
  if (ev.loose) return [];
  const p = props ?? {};
  const problems: string[] = [];
  for (const [name, spec] of Object.entries(ev.props)) {
    const present = Object.prototype.hasOwnProperty.call(p, name) && p[name] !== undefined;
    if (!present) {
      if (!spec.optional) problems.push(`missing required prop "${name}"`);
      continue;
    }
    const val = p[name];
    const actual = kindOf(val);
    switch (spec.kind) {
      case "string":
      case "number":
      case "boolean":
        if (actual !== spec.kind) problems.push(`prop "${name}" should be ${spec.kind}, got ${actual}`);
        break;
      case "enum-string":
        if (actual !== "string") problems.push(`prop "${name}" should be a string, got ${actual}`);
        else if (spec.values && !spec.values.includes(val as string))
          problems.push(`prop "${name}" = ${JSON.stringify(val)} is not one of {${spec.values.join(", ")}}`);
        break;
      case "enum-number":
        if (actual !== "number") problems.push(`prop "${name}" should be a number, got ${actual}`);
        else if (spec.values && !spec.values.includes(val as number))
          problems.push(`prop "${name}" = ${String(val)} is not one of {${spec.values.join(", ")}}`);
        break;
      default:
        break;
    }
  }
  return problems;
}
