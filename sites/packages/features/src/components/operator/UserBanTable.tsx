import {
  shortAddress,
  type UserBan,
} from "@data/lib/catalyst/admin/user-bans";

export type UserBanTableProps = {
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

function hueFor(addr: string): number {
  let h = 0;
  for (let i = 0; i < addr.length; i++) h = (h * 31 + addr.charCodeAt(i)) % 360;
  return h;
}

export default function UserBanTable({ bans, onLift, onSelect }: UserBanTableProps) {
  return (
    <div className="au__tablewrap">
      <table className="au__table" aria-label="Active global bans">
        <thead>
          <tr>
            <th className="au__th">User</th>
            <th className="au__th">Reason</th>
            <th className="au__th">Banned by</th>
            <th className="au__th">Expires</th>
            <th className="au__th au__th--center">Action</th>
          </tr>
        </thead>
        <tbody>
          {bans.map((b) => (
            <tr
              key={b.id}
              className="au-row"
              onClick={onSelect ? () => onSelect(b.bannedAddress) : undefined}
            >
              <td className="au-cell au-cell--user">
                <span
                  className="au-avatar u-avatar"
                  style={{ "--sz": "40px", "--hue": hueFor(b.bannedAddress) } as React.CSSProperties}
                  aria-hidden="true"
                />
                <span className="au-cell__addr">{shortAddress(b.bannedAddress)}</span>
                {b.name ? <span className="au-cell__name">{` (${b.name})`}</span> : null}
              </td>
              <td className="au-cell">{b.reason}</td>
              <td className="au-cell">{shortAddress(b.bannedBy)}</td>
              <td className="au-cell">{expiryLabel(b.expiresAt)}</td>
              <td className="au-cell au-cell--center">
                <button
                  type="button"
                  className="au-btn au-btn--secondary"
                  onClick={(e) => {
                    e.stopPropagation();
                    onLift(b.bannedAddress);
                  }}
                >
                  Lift ban
                </button>
              </td>
            </tr>
          ))}
          {bans.length === 0 && (
            <tr>
              <td className="au-cell au-cell--center au-cell--empty" colSpan={5}>
                No active global bans
              </td>
            </tr>
          )}
        </tbody>
      </table>
    </div>
  );
}
