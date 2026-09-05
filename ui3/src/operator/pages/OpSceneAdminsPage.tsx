import AdControlNotice, { AdBlockedAction } from "../../admin/pages/AdControlNotice";
import SitesChrome from "../../web/frames/SitesChrome";
import OpPlacePicker, { type OpPickablePlace } from "../components/OpPlacePicker";
import "../../web/pages/stwhatsonadminusers.css";
import "../components/sceneadmins.css";

/**
 * Scene admins -- a public place list, and a grant list that cannot be read.
 *
 * These are two different things and the page must not blur them.
 *
 * The place list is public: `GET /places/api/places?owner=` is
 * `catalyrst-places/src/handlers/places.rs:66-73` (`auth_address_optional`, no
 * gate). The address is a filter, not an identity.
 *
 * The grants are BLOCK. The server-side checks are real and correct --
 *   list    `catalyrst-comms/src/handlers/scene_admin.rs:56-62`
 *   grant   `catalyrst-comms/src/handlers/scene_admin.rs:123-131`
 *   revoke  `catalyrst-comms/src/handlers/scene_admin.rs:145-157`
 *           -> `ports/scene_perms.rs:16-114`, which denies on pool failure
 *              (`:27-34`)
 * -- but they are unreachable from this node: there is no nginx `location` for
 * `/scene-admin`, and the correct public path `/comms/scene-admin` is used
 * nowhere. Adding that edge route is a deployment change, not part of a UI
 * change.
 *
 * So the wizard is not rendered. It previously showed an empty admin list
 * (`scene-admins.server.ts` hardcoded `grants: []`), which read as "this place
 * has no scene admins", above Add and Revoke buttons that could not work.
 */
export type OpSceneAdminsPageProps = {
  /** The address the public place filter was run for. Not an identity claim. */
  viewedAddress: string;
  /** True when that address is the built-in demo value, not the viewer. */
  isDemo: boolean;
  places: OpPickablePlace[];
  /** Non-null when even the public place read failed. */
  placesUnavailableReason: string | null;
  selectedPlaceId: string | null;
  onSelectPlace: (placeId: string) => void;
  /** Why the grant list and the grant/revoke controls are unavailable. */
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
