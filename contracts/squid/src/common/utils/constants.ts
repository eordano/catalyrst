// Placeholder for a contract a network has no deployment of. The processors register every log
// filter unconditionally and an empty address array means ANY address to Subsquid, so a version
// that is not deployed yet must still point somewhere; nothing can emit from the zero address.
export const Null = "0x0000000000000000000000000000000000000000";
