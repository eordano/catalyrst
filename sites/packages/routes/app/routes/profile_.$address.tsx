import type { AgentMarkdownHandle } from "@data/lib/agent/markdown";

export { loader, default } from "./landings.profile";

export const handle = { agentMarkdown: "profileDetail" } satisfies AgentMarkdownHandle;
