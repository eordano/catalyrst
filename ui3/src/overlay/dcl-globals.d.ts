import type { BridgeApi, DeployIdentity } from "./bridge";
import type { NativeHostMessage } from "../generated/bridge/NativeHostMessage";

declare global {
  interface Window {
    dclBridge?: BridgeApi;
    // Host-private members (_push/_event/_reply) are deliberately undeclared:
    // ui3 must not be able to call them.
    __dclNativeHost?: { post: (msg: NativeHostMessage) => void };
    dclDeployIdentity?: DeployIdentity;
    __dclHasIdentity?: boolean;
    __dclConsumedOpenPanelNonce?: number;

    dclDeferStart?: boolean;
    dclEngineReady?: boolean;
    dclEngineStart?: () => void | Promise<void>;

    dclLoadingProgress?: number;

    __emojiFontBytes?: Uint8Array;
    __assetsBundle?: Uint8Array;

    engine_console_command?: (command: string) => Promise<string>;

    dclDracoReady?: Promise<boolean>;
    dclDracoDecode?: (
      srcBytes: Uint8Array,
      attrMap: Record<string, string>,
    ) => Promise<{
      indices: Uint32Array;
      numPoints: number;
      attributes: Record<string, { data: Float32Array; components: number }>;
    }>;

    __DCL_PUBLIC__?: { thirdwebClientId?: string; thirdwebSignProxy?: string };
    __DCL_AUTH_HEADERS__?: Record<string, string>;

    __srch?: unknown;
  }
}
