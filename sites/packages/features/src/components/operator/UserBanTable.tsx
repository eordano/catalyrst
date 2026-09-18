import Button from "@ui/atoms/Button";
import { Avatar } from "@ui/atoms/primitives";
import "@ui/admin/admin.css";

import {
  shortAddress,
  type UserBan,
} from "@data/lib/catalyst/admin/user-bans";

type UserBanTableProps = {
  bans: UserBan[];
  onLift: (address: string) => void;
  onSelect?: (address: string) => void;
};

function expiryLabel(expiresAt: string | null): string {
  if (!expiresAt) return "Permanent";
  const d = new Date(expiresAt);
  if (Number.isNaN(d.getTime())) return "\u{2014}";
  return d.toLocaleString("en-US", {
    day: "numeric",
    month: "short",
    year: "numeric",
    timeZone: "UTC",
  });
}

export default function UserBanTable({ bans, onLift, onSelect }: UserBanTableProps) {
  return (
    <div className="adm-scroll">
      <table className="adm-table" aria-label="Active global bans">
        <thead>
          <tr>
            <th>User</th>
            <th>Reason</th>
            <th>Banned by</th>
            <th>Expires</th>
            <th className="is-center">Action</th>
          </tr>
        </thead>
        <tbody>
          {bans.map((b) => (
            <tr
              key={b.id}
              className={onSelect ? "is-link" : undefined}
              onClick={onSelect ? () => onSelect(b.bannedAddress) : undefined}
            >
              <td className="is-nowrap">
                <Avatar seed={b.bannedAddress} size={40} />
                <span className="adm-mono">{shortAddress(b.bannedAddress)}</span>
                {b.name ? <span className="adm-dim">{` (${b.name})`}</span> : null}
              </td>
              <td>{b.reason}</td>
              <td className="adm-mono">{shortAddress(b.bannedBy)}</td>
              <td>{expiryLabel(b.expiresAt)}</td>
              <td className="is-center">
                <Button
                  variant="secondary"
                  size="sm"
                  tone="success"
                  onClick={(e) => {
                    e.stopPropagation();
                    onLift(b.bannedAddress);
                  }}
                >
                  Lift ban
                </Button>
              </td>
            </tr>
          ))}
          {bans.length === 0 && (
            <tr>
              <td className="is-empty" colSpan={5}>
                No active global bans
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
