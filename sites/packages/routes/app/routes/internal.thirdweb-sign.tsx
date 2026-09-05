import { data } from "react-router";

import {
  signMessageEnclave,
  signTypedDataEnclave,
  ThirdwebError,
  type Eip712TypedData,
} from "@data/lib/auth/thirdweb/api";
import { LOGIN_CHAIN_ID, thirdwebSecretKey } from "@data/lib/auth/thirdweb/config";

import type { Route } from "./+types/internal.thirdweb-sign";


type SignBody =
  | { kind: "message"; token: string; from: string; message: string; chainId?: number }
  | {
      kind: "typedData";
      token: string;
      from: string;
      typedData: Eip712TypedData;
      chainId: number;
    };

export async function action({ request }: Route.ActionArgs) {
  if (request.method !== "POST") {
    return data({ error: "method not allowed" }, { status: 405 });
  }
  const secretKey = thirdwebSecretKey();
  if (!secretKey) {
    return data(
      {
        error:
          "Sign-in is not fully configured on this server (THIRDWEB_SECRET_KEY unset).",
      },
      { status: 503 },
    );
  }

  let body: SignBody;
  try {
    body = (await request.json()) as SignBody;
  } catch {
    return data({ error: "invalid JSON body" }, { status: 400 });
  }
  if (!body || typeof body.token !== "string" || typeof body.from !== "string") {
    return data({ error: "missing token/from" }, { status: 400 });
  }

  try {
    let signature: string;
    if (body.kind === "message") {
      if (typeof body.message !== "string") {
        return data({ error: "missing message" }, { status: 400 });
      }
      signature = await signMessageEnclave(
        body.token,
        body.from,
        body.message,
        body.chainId ?? LOGIN_CHAIN_ID,
        request.signal,
        secretKey,
      );
    } else if (body.kind === "typedData") {
      if (!body.typedData || typeof body.chainId !== "number") {
        return data({ error: "missing typedData/chainId" }, { status: 400 });
      }
      signature = await signTypedDataEnclave(
        body.token,
        body.from,
        body.typedData,
        body.chainId,
        request.signal,
        secretKey,
      );
    } else {
      return data({ error: "unknown sign kind" }, { status: 400 });
    }
    return { signature };
  } catch (err) {
    if (err instanceof ThirdwebError) {
      return data({ error: err.message }, { status: err.status || 502 });
    }
    return data(
      { error: (err as Error)?.message ?? "sign failed" },
      { status: 502 },
    );
  }
}
