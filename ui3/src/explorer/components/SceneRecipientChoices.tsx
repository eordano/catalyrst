import { useQuery } from "@tanstack/react-query";
import { fetchProfile } from "../../data/catalyst/profile";
import type { SceneRecipient, SceneRecipientRole } from "../../data/catalyst/sceneOwner";
import { qk, STALE } from "../../data/queryKeys";
import { truncateAddress } from "../../data/format";

function Recipient({ recipient, selected, disabled, onSelect }: {
  recipient: SceneRecipient; selected: boolean; disabled: boolean; onSelect: () => void;
}) {
  const profile = useQuery({
    queryKey: qk.profile(recipient.address ?? ""),
    queryFn: ({ signal }) => fetchProfile(recipient.address, { signal }),
    enabled: !!recipient.address,
    staleTime: STALE.profile,
    retry: false,
  });
  const name = profile.data?.name?.trim();
  return <label className="soa__recipient" data-selected={selected}>
    <input type="radio" name="scene-feedback-recipient" aria-label={recipient.label} checked={selected} disabled={disabled || !recipient.address} onChange={onSelect} />
    <span className="soa__recipient-body">
      <strong>{name || (profile.isFetching ? "Loading profile\u2026" : truncateAddress(recipient.address!))}</strong>
      <span className="soa__recipient-role">{recipient.label}</span><small>{recipient.source}</small>
    </span>
  </label>;
}

export default function SceneRecipientChoices({ recipients, selected, disabled, loading, onSelect }: {
  recipients: SceneRecipient[]; selected?: SceneRecipientRole; disabled: boolean; loading: boolean; onSelect: (role: SceneRecipientRole) => void;
}) {
  const available = recipients.filter(recipient => recipient.address);
  return <fieldset className="soa__recipients" disabled={disabled}>
    <legend>Recipient</legend>
    {available.map(recipient => <Recipient key={recipient.role} recipient={recipient} selected={recipient.role === selected} disabled={disabled} onSelect={() => onSelect(recipient.role)} />)}
    {loading && <p role="status">Finding scene recipients&hellip;</p>}
    {!loading && available.length === 0 && <p role="status">No recipients are available for this scene.</p>}
  </fieldset>;
}
