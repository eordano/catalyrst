import { useEffect, useRef, useState, type CSSProperties } from "react";
import { useNavigate, useSearchParams } from "react-router";

import {
  clearVerifiedWalletCookie,
  serializeVerifiedWalletCookie,
} from "@core/lib/experiments/assign";
import { useAuth } from "@data/lib/auth/context";
import { getWalletForToken } from "@data/lib/auth/thirdweb/index";

import type { Route } from "./+types/auth.callback";

export function meta(_args: Route.MetaArgs) {
  return [{ title: "Signing you in \u{2014} Decentraland" }];
}

export async function action({ request }: Route.ActionArgs) {
  if (request.method !== "POST") {
    return Response.json({ ok: false }, { status: 405 });
  }

  let token = "";
  let signout = false;
  try {
    const body = (await request.json()) as { token?: unknown; signout?: unknown };
    if (typeof body?.token === "string") token = body.token;
    if (body?.signout === true) signout = true;
  } catch {
  }

  if (signout) {
    return Response.json(
      { ok: true },
      { headers: { "Set-Cookie": clearVerifiedWalletCookie() } },
    );
  }

  const address = token ? await getWalletForToken(token, request.signal) : null;
  if (!address) {
    return Response.json(
      { ok: false },
      { status: 401, headers: { "Set-Cookie": clearVerifiedWalletCookie() } },
    );
  }

  return Response.json(
    { ok: true, address },
    {
      status: 200,
      headers: { "Set-Cookie": serializeVerifiedWalletCookie(address) },
    },
  );
}

function extractToken(authResultRaw: string): string | null {
  try {
    const parsed = JSON.parse(authResultRaw) as {
      storedToken?: { cookieString?: string; jwtToken?: string };
      token?: string;
      cookieString?: string;
    };
    return (
      parsed.storedToken?.cookieString ??
      parsed.storedToken?.jwtToken ??
      parsed.token ??
      parsed.cookieString ??
      null
    );
  } catch {
    return null;
  }
}

export default function AuthCallbackRoute() {
  const auth = useAuth();
  const navigate = useNavigate();
  const [params] = useSearchParams();
  const [error, setError] = useState<string | null>(null);
  const ran = useRef(false);

  useEffect(() => {
    if (ran.current) return;
    ran.current = true;

    const next = params.get("next") || "/";
    const authResultRaw = params.get("authResult");
    if (!authResultRaw) {
      setError("Sign-in didn't return a result. Please try again.");
      return;
    }
    const token = extractToken(authResultRaw);
    if (!token) {
      setError("Couldn't read the sign-in result. Please try again.");
      return;
    }

    void (async () => {
      const address = await getWalletForToken(token);
      if (!address) {
        setError("Your sign-in expired before we could finish. Please try again.");
        return;
      }
      const identity = await auth.completeInAppSession({
        token,
        walletAddress: address,
      });
      if (identity) {
        await fetch(window.location.pathname, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({ token }),
        }).catch(() => null);
        navigate(next, { replace: true });
      } else {
        setError(auth.error ?? "Couldn't finish signing you in.");
      }
    })();
  }, [auth, navigate, params]);

  return (
    <main style={wrap}>
      {error ? (
        <>
          <h1 style={{ fontSize: 22, margin: 0 }}>Sign-in failed</h1>
          <p style={{ color: "rgba(232,232,240,0.7)" }}>{error}</p>
          <a href="/" style={link}>
            &#x2190; Back to Decentraland
          </a>
        </>
      ) : (
        <p style={{ color: "rgba(232,232,240,0.7)" }}>Finishing sign-in&#x2026;</p>
      )}
    </main>
  );
}

const wrap: CSSProperties = {
  maxWidth: 520,
  margin: "0 auto",
  padding: "80px 24px",
  color: "#e8e8f0",
  font: "15px/1.6 system-ui, sans-serif",
  textAlign: "center",
};

const link: CSSProperties = {
  color: "#ff2d55",
  textDecoration: "none",
  font: "600 14px/1 system-ui, sans-serif",
};
