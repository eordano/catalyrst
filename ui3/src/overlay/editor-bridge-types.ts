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

export type EditorBridgeReply =
  | {
      type: "dcl-bridge-reply";
      action: EditorBridgeAction;
      requestId?: string | number;
      ok: true;
      result: string;
    }
  | {
      type: "dcl-bridge-reply";
      action: EditorBridgeAction;
      requestId?: string | number;
      ok: false;
      error: string;
    };
