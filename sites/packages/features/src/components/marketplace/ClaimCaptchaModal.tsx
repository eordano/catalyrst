import React, { useCallback, useEffect, useRef, useState } from "react";

import {
  claimCredits,
  fetchClaimCaptcha,
} from "@data/lib/catalyst/marketplace/credits";
import type { AuthIdentity } from "@data/lib/auth/types";

type Props = {
  identity: AuthIdentity;
  claimable: number;
  onSuccess: (granted: number) => void;
  onClose: () => void;
};

export default function ClaimCaptchaModal({
  identity,
  claimable,
  onSuccess,
  onClose,
}: Props) {
  const [imgUrl, setImgUrl] = useState<string | null>(null);
  const [x, setX] = useState(50);
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [blocked, setBlocked] = useState(false);
  const urlRef = useRef<string | null>(null);

  const loadCaptcha = useCallback(() => {
    setError(null);
    setImgUrl(null);
    fetchClaimCaptcha(identity)
      .then((blob) => {
        if (urlRef.current) URL.revokeObjectURL(urlRef.current);
        urlRef.current = URL.createObjectURL(blob);
        setImgUrl(urlRef.current);
      })
      .catch(() => setError("Couldn't load the captcha. Try again."));
  }, [identity]);

  useEffect(() => {
    loadCaptcha();
    return () => {
      if (urlRef.current) URL.revokeObjectURL(urlRef.current);
    };
  }, [loadCaptcha]);

  async function submit() {
    if (busy || blocked) return;
    setBusy(true);
    setError(null);
    try {
      const res = await claimCredits(identity, x);
      if (res.ok && res.credits_granted > 0) {
        onSuccess(res.credits_granted);
      } else if (res.isBlockedForClaiming) {
        setBlocked(true);
        setError("Claiming is currently blocked for this account.");
      } else if (res.ok) {
        onSuccess(0);
      } else {
        setError("The marker wasn't on the notch \u{2014} try again.");
        loadCaptcha();
      }
    } catch {
      setError("Claim failed. Check your connection and try again.");
      loadCaptcha();
    } finally {
      setBusy(false);
    }
  }

  return (
    <div role="dialog" aria-modal="true" aria-label="Claim credits" style={overlayStyle}>
      <div style={cardStyle}>
        <h3 style={{ margin: 0, fontSize: 18 }}>
          Claim {claimable > 0 ? `${claimable} ` : ""}Credits
        </h3>
        <p style={hintStyle}>
          Drag the slider until the white line sits on the dark notch, then
          claim.
        </p>

        <div style={{ position: "relative", width: 320, margin: "0 auto" }}>
          {imgUrl ? (
            <img
              src={imgUrl}
              alt="Slider captcha: a noise strip with a dark notch"
              width={320}
              height={120}
              style={{ display: "block", borderRadius: 8, imageRendering: "pixelated" }}
              data-testid="mc-captcha-img"
            />
          ) : (
            <div style={{ width: 320, height: 120, borderRadius: 8, background: "rgba(255,255,255,0.06)" }} />
          )}
          <div
            aria-hidden
            style={{
              position: "absolute",
              top: 0,
              bottom: 0,
              left: `${(x / 100) * 100}%`,
              width: 2,
              marginLeft: -1,
              background: "#fff",
              boxShadow: "0 0 4px rgba(0,0,0,0.9)",
              pointerEvents: "none",
            }}
          />
        </div>

        <input
          type="range"
          min={0}
          max={100}
          step={0.5}
          value={x}
          disabled={busy || blocked || !imgUrl}
          onChange={(e) => setX(Number(e.target.value))}
          style={{ width: 320, margin: "12px auto 0", display: "block" }}
          aria-label="Captcha slider position"
          data-testid="mc-captcha-slider"
        />

        {error && (
          <p role="alert" style={{ ...hintStyle, color: "#ff7a90" }}>
            {error}
          </p>
        )}

        <div style={{ display: "flex", gap: 8, justifyContent: "center", marginTop: 16 }}>
          <button type="button" onClick={onClose} style={ghostBtnStyle} disabled={busy}>
            Cancel
          </button>
          <button
            type="button"
            onClick={submit}
            style={primaryBtnStyle}
            disabled={busy || blocked || !imgUrl}
            data-testid="mc-captcha-submit"
          >
            {busy ? "Claiming\u{2026}" : "Claim"}
          </button>
        </div>
      </div>
    </div>
  );
}

const overlayStyle: React.CSSProperties = {
  position: "fixed",
  inset: 0,
  background: "rgba(0,0,0,0.6)",
  display: "flex",
  alignItems: "center",
  justifyContent: "center",
  zIndex: 1000,
};

const cardStyle: React.CSSProperties = {
  width: 380,
  maxWidth: "calc(100vw - 32px)",
  padding: 24,
  borderRadius: 16,
  background: "#1b1b23",
  border: "1px solid rgba(255,255,255,0.10)",
  color: "rgba(255,255,255,0.92)",
  textAlign: "center",
};

const hintStyle: React.CSSProperties = {
  fontSize: 13,
  lineHeight: 1.5,
  color: "rgba(255,255,255,0.65)",
  margin: "8px 0 16px",
};

const primaryBtnStyle: React.CSSProperties = {
  padding: "10px 26px",
  borderRadius: 999,
  border: 0,
  background: "var(--brand-cta)",
  color: "#fff",
  fontSize: 14,
  fontWeight: 600,
  cursor: "pointer",
};

const ghostBtnStyle: React.CSSProperties = {
  padding: "10px 22px",
  borderRadius: 999,
  border: "1px solid rgba(255,255,255,0.25)",
  background: "transparent",
  color: "rgba(255,255,255,0.85)",
  fontSize: 14,
  cursor: "pointer",
};
