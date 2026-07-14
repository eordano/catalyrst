/**
 * Where a row on a Foundry surface came from.
 *
 * `imported` — read out of a source we do not own (the worlds mirror, a design
 * document someone wrote elsewhere). `recorded` — produced by an execution that
 * actually ran (a bot bench, a copilot exchange, a program-drafted document).
 * `visitor` — someone on this site did it.
 *
 * There is deliberately no fourth value: a row that fits none of these has no
 * business being rendered.
 */
export type FdProvenance = "imported" | "recorded" | "visitor";
