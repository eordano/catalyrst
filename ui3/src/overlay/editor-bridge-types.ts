export type EditorBridgeAction = "FreezeScene" | "UnfreezeScene" | "TickScene";

export type EditorBridgeRequest =
  | {
      type: "dcl-bridge";
      action: "FreezeScene" | "UnfreezeScene";
      requestId?: string | number;
    }
  | {
      type: "dcl-bridge";
      action: "TickScene";
      count?: number;
      requestId?: string | number;
    };

