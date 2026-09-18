import type { BridgeApi, DeployIdentity } from "./bridge";
import type { NativeHostMessage } from "../generated/bridge/NativeHostMessage";

declare global {
  interface Window {
    dclBridge?: BridgeApi;
    __dclNativeHost?: { post: (msg: NativeHostMessage) => void };
    dclDeployIdentity?: DeployIdentity;
    __dclHasIdentity?: boolean;
    __dclConsumedOpenPanelNonce?: number;

    dclDeferStart?: boolean;
    dclEngineReady?: boolean;
    dclEngineConnectLobby?: () => void | Promise<void>;
    dclEngineStart?: (destination?: { realm?: string; parcel?: [number, number] }) => void | Promise<void>;

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

    __DCL_PUBLIC__?: { thirdwebClientId?: string; thirdwebSignProxy?: string; docsBase?: string };
    __DCL_AUTH_HEADERS__?: Record<string, string>;

    __srch?: unknown;
  }
}
