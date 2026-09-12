import AdControlNotice, { AdBlockedAction } from "../../admin/pages/AdControlNotice";
import SitesChrome from "../../web/frames/SitesChrome";
import OpPlacePicker, { type OpPickablePlace } from "../components/OpPlacePicker";
import "../../web/pages/stwhatsonadminusers.css";
import "../components/sceneadmins.css";

export type OpSceneAdminsPageProps = {
  viewedAddress: string;
  isDemo: boolean;
  places: OpPickablePlace[];
  placesUnavailableReason: string | null;
  selectedPlaceId: string | null;
  onSelectPlace: (placeId: string) => void;
  grantsMessage: string;
  grantsServerCheck: string | null;
  grantsFix?: string;
};

export default function OpSceneAdminsPage({
  viewedAddress,
  isDemo,
  places,
  placesUnavailableReason,
  selectedPlaceId,
  onSelectPlace,
  grantsMessage,
  grantsServerCheck,
  grantsFix = undefined,
}: OpSceneAdminsPageProps) {
  return (
    <SitesChrome active="create">
      <main className="sa-route">
        <div className="sa__head">
          <h1 className="sa__title">Scene admins</h1>
          <p className="sa__sub">
            Viewing places registered to <code>{viewedAddress}</code>
            {isDemo ? " \u{2014} demo address, not you" : ""}. The place list is public
            data (<code>GET /places/api/places?owner=</code>); the address is a
            filter and grants nothing.
          </p>
        </div>

        {placesUnavailableReason ? (
          <p className="sa-route__demo" role="alert">
            The public place list could not be read: {placesUnavailableReason}
          </p>
        ) : (
          <OpPlacePicker
            places={places}
            selectedId={selectedPlaceId}
            onSelect={onSelectPlace}
            owner={viewedAddress}
          />
        )}

        <AdControlNotice
          title="Scene-admin grants"
          message={grantsMessage}
          serverCheck={grantsServerCheck}
          fix={grantsFix}
        />

        <div className="sa__toolbar">
          <AdBlockedAction label="Add scene admin" reason={grantsMessage} />
          <AdBlockedAction label="Revoke scene admin" reason={grantsMessage} />
        </div>
      </main>
    </SitesChrome>
  );
}
