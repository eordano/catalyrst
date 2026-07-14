import type { ClientEvents } from "./events/client";
import type { CreatorHubEvents } from "./events/creator-hub";
import type { GovernanceEvents } from "./events/governance";
import type { LandingsEvents } from "./events/landings";
import type { MarketplaceEvents } from "./events/marketplace";
import type { OperatorAdminEvents } from "./events/operator-admin";
import type { MiscEvents } from "./events/misc";

export type TelemetryEvents = ClientEvents &
  CreatorHubEvents &
  GovernanceEvents &
  LandingsEvents &
  MarketplaceEvents &
  OperatorAdminEvents &
  MiscEvents;

export type TelemetryEventName = keyof TelemetryEvents;
