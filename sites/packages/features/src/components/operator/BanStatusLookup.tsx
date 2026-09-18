import { useState } from "react";

import Button from "@ui/atoms/Button";
import "@ui/admin/admin.css";

import {
  isAddress,
  loadBanStatus,
  normalizeAddress,
  shortAddress,
  type PublicUserBan,
} from "@data/lib/catalyst/admin/user-bans";

type BanStatusLookupProps = {
  initialAddress?: string;
  onLookup: (args: { address: string; isBanned: boolean; ban: PublicUserBan | null }) => void;
  onAct?: (args: { address: string; isBanned: boolean }) => void;
};

type LookupResult = { address: string; isBanned: boolean; ban: PublicUserBan | null };

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
  const errorClass = invalidFormat ? " is-error" : "";

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
    <div className={"adm-field" + errorClass} role="search" aria-label="Look up ban status">
      <label className="adm-field__label" htmlFor="op-lookup">
        Look up an address
      </label>
      <div className="adm-actions adm-actions--start">
        <input
          id="op-lookup"
          className="adm-input"
          placeholder={"0x\u{2026}"}
          value={address}
          onChange={(e) => setAddress(e.target.value)}
          onKeyDown={(e) => {
            if (e.key === "Enter") doLookup();
          }}
          aria-label="Wallet address"
        />
        <Button onClick={doLookup} disabled={!valid || loading}>
          {loading ? "Looking up\u{2026}" : "Look up"}
        </Button>
      </div>
      <span className={"adm-field__help" + errorClass}>
        {invalidFormat ? "Enter a valid Ethereum address" : " "}
      </span>

      {error && (
        <div className="adm-notice" data-tone="bad" role="alert">
          <p>{error}</p>
        </div>
      )}

      {result && !error && (
        <div className="adm-notice" role="status" aria-live="polite">
          <p>
            {shortAddress(result.address)} is{" "}
            <strong>{result.isBanned ? "BANNED" : "not banned"}</strong>
            {result.ban ? ` \u{2014} ${result.ban.reason}` : ""}
          </p>
          {onAct && (
            <div className="adm-actions adm-actions--start">
              <Button
                variant="secondary"
                size="sm"
                onClick={() => onAct({ address: result.address, isBanned: result.isBanned })}
              >
                Act on this user
              </Button>
            </div>
          )}
        </div>
      )}
    </div>
  );
}
