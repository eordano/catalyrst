import type { FdRoleCardVM } from "./FdRoleCard";

export type FdRoleDestination = "play" | "copilot" | "console";

export type FdRoleDoor = FdRoleCardVM & { destinationId: FdRoleDestination };

// Display copy says "Operate"/"operator"; the internal role id stays 'admin' so
// the grants and consents already recorded keep working.
export const FD_ROLES: readonly FdRoleDoor[] = [
  {
    id: "start",
    role: "Start",
    who: "the player's door",
    title: "Show up and play.",
    body: "Walk into a live world and play something a person made.",
    destination: "Opens the games on the shelf.",
    destinationId: "play",
    href: "/foundry/play",
    cta: "Start playing",
  },
  {
    id: "create",
    role: "Create",
    who: "the builder's door",
    title: "Build something your friends will show up for.",
    body: "Draft a game doc with the copilot; the program builds from it.",
    destination:
      "Opens the copilot page. Its door takes an operator or host invite; the operator password is the fallback.",
    destinationId: "copilot",
    href: "/foundry/copilot",
    cta: "Start building",
  },
  {
    id: "admin",
    role: "Operate",
    who: "the operator's door",
    title: "See how the program runs.",
    body: "The checks each game is put through, and what the copilot costs.",
    destination: "Opens the console. Hosting your own world is not built yet.",
    destinationId: "console",
    href: "/foundry/console",
    cta: "Open the console",
  },
];
