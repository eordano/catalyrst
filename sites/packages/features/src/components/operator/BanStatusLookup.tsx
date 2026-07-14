import { useState } from "react";

import {
  isAddress,
  loadBanStatus,
  normalizeAddress,
  shortAddress,
  type UserBan,
} from "@data/lib/catalyst/admin/user-bans";

export type BanStatusLookupProps = {
  initialAddress?: string;
  onLookup: (args: { address: string; isBanned: boolean; ban: UserBan | null }) => void;
  onAct?: (args: { address: string; isBanned: boolean }) => void;
};

type LookupResult = { address: string; isBanned: boolean; ban: UserBan | null };

export default function BanStatusLookup({
  initialAddress = "",
  onLookup,
  onAct,
}: BanStatusLookupProps) {
  const [address, setAddress] = useState(initialAddress);
  const [result, setResult] = useState<LookupResult | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const valid = isAddress(address);
  const invalidFormat = address.trim().length > 0 && !valid;

  async function doLookup() {
    if (!valid || loading) return;
    const norm = normalizeAddress(address);
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const status = await loadBanStatus(norm);
      const out: LookupResult = { address: norm, isBanned: status.isBanned, ban: status.ban };
      setResult(out);
      onLookup(out);
    } catch {
      setError("Couldn't reach the ban-status service. Try again.");
    } finally {
      setLoading(false);
    }
  }

  return (
    <div className="au-field" role="search" aria-label="Look up ban status">
      <label className="au-field__label" htmlFor="op-lookup">
        Look up an address
      </label>
      <div style={{ display: "flex", gap: 8 }}>
        <input
          id="op-lookup"
          className={"au-field__input" + (invalidFormat ? " is-error" : "")}
          placeholder={"0x\u{2026}"}
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") doLookup();
          }}
          aria-label="Wallet address"
          style={{ flex: 1 }}
        />
        <button
          type="button"
          className="au-btn au-btn--primary"
          onClick={doLookup}
          disabled={!valid || loading}
        >
          {loading ? "Looking up\u{2026}" : "Look up"}
        </button>
      </div>
      <span className={"au-field__help" + (invalidFormat ? " is-error" : "")}>
        {invalidFormat ? "Enter a valid Ethereum address" : " "}
      </span>

      {error && (
        <div className="au-alert au-alert--error" role="alert" style={{ marginTop: 8 }}>
          <span className="au-alert__msg">{error}</span>
        </div>
      )}

      {result && !error && (
        <div className="au-alert" role="status" aria-live="polite" style={{ marginTop: 8 }}>
          <span className="au-alert__msg">
            {shortAddress(result.address)} is{" "}
            <strong>{result.isBanned ? "BANNED" : "not banned"}</strong>
            {result.ban ? ` \u{2014} ${result.ban.reason}` : ""}
          </span>
          {onAct && (
            <button
              type="button"
              className="au-btn au-btn--secondary"
              onClick={() => onAct({ address: result.address, isBanned: result.isBanned })}
            >
              Act on this user
            </button>
          )}
        </div>
      )}
    </div>
  );
}
