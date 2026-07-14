import { useNavigate } from "react-router";

import { useFriends } from "../../data/hooks/useFriends";
import { requestFriendAction } from "../../data/hooks/friendActions";
import { relationshipOf } from "../../data/hooks/relationship";
import { useBridgeState } from "../../overlay/bridge";
import {
  ProfileCardPresentation,
  type ProfileCardUser,
} from "./ProfileCardPresentation";

export type ProfileCardProps = {
  user: ProfileCardUser;
  x: number;
  y: number;
  onMention?: (name: string) => void;
  onViewPassport?: (user: ProfileCardUser) => void;
  onClose: () => void;
};

export default function ProfileCard({
  user,
  x,
  y,
  onMention,
  onViewPassport,
  onClose,
}: ProfileCardProps) {
  const navigate = useNavigate();
  const identity = useBridgeState((s) => s.identity);
  const { friends, received, sent, blocked } = useFriends();
  const relationship = relationshipOf(friends, received, sent, blocked, user.address);

  const viewPassport =
    onViewPassport ??
    ((u: ProfileCardUser) => navigate(`/passport?address=${encodeURIComponent(u.address)}`));

  return (
    <ProfileCardPresentation
      user={user}
      x={x}
      y={y}
      me={{ address: identity?.address ?? undefined }}
      relationship={relationship}
      onFriendAction={(op, address) => requestFriendAction(op, address)}
      onMention={onMention}
      onViewPassport={viewPassport}
      onClose={onClose}
    />
  );
}
