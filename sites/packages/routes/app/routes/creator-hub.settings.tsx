import { useNavigate } from "react-router";

import CreatorHubChrome from "@ui/creatorhub/frames/CreatorHubChrome";
import ChAppSettingsTabbedSections from "@ui/creatorhub/components/ChAppSettingsTabbedSections";
import "@ui/creatorhub/components/chappsettingstabbedsections.css";

import { useAuth } from "@data/lib/auth/index";
import { openSignIn } from "@features/components/auth/signin-store";
import { useProfileName } from "@data/lib/auth/use-profile-name";
import { creatorHubMeta } from "@core/lib/seo/creator-hub-meta";

export const meta = () => creatorHubMeta("Preferences");


export async function loader() {
  return {};
}

export default function CreatorHubSettings() {
  const { isConnected, address } = useAuth();
  const name = useProfileName(address, isConnected);
  const navigate = useNavigate();

  return (
    <CreatorHubChrome
      active={null}
      signedIn={isConnected}
      account={address ?? ""}
      name={name}
      onSignIn={openSignIn}
    >
      <ChAppSettingsTabbedSections
        open
        web
        onClose={() => {
          if (
            typeof window !== "undefined" &&
            (window.history.state?.idx ?? 0) > 0
          ) {
            navigate(-1);
          } else {
            navigate("/create");
          }
        }}
      />
    </CreatorHubChrome>
  );
}
