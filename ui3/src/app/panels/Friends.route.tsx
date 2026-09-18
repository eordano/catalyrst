import { useNavigate } from "react-router";

import Friends from "../../explorer/pages/Friends";
import { useFriends } from "../../data/hooks/useFriends";
import { useBridgeState } from "../../overlay/bridge";

type FriendsPanelProps = {
  floating?: boolean;
  onClose?: () => void;
};

export default function FriendsPanel({ floating = false, onClose }: FriendsPanelProps = {}) {
  const navigate = useNavigate();
  const identity = useBridgeState((s) => s.identity);
  const { friends, received, sent, blocked, isPending } = useFriends();

  return (
    <Friends
      key={identity.address ?? "guest"}
      loading={isPending}
      initialSection="friends"
      floating={floating}
      isGuest={identity.isGuest}
      onClose={onClose || (() => navigate("/"))}
      friends={[...friends]}
      received={[...received]}
      sent={[...sent]}
      blocked={[...blocked]}
    />
  );
}
