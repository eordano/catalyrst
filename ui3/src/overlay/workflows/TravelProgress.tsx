import type { TravelStatus } from "../../generated/bridge/TravelStatus";
import Spinner from "../../atoms/Spinner";
import { useLifecycleCommand } from "../useLifecycleCommand";

export function isTravelActive(travel: TravelStatus | null | undefined): travel is TravelStatus {
  return !!travel && ["requested", "resolving", "preparing", "placing"].includes(travel.phase);
}

export default function TravelProgress({ travel }: { travel: TravelStatus }) {
  const cancel = useLifecycleCommand();
  return (
    <div className="map__teleport">
      <div className="map__teleportcard">
        <div className="map__teleportspinner"><Spinner size={40} color="var(--explore-orange, #ff7a3d)" aria-hidden="true" /></div>
        <div role="status" aria-live="polite">
          <p className="map__teleporttitle">{"Teleporting\u2026"}</p>
          <p className="map__teleportsub">{travel.blockingReason ?? "Preparing your destination"}</p>
          <p className="map__teleportnote">{travel.parcel ? `Destination: ${travel.parcel.join(", ")} \u00b7 ${travel.realm}` : travel.realm}</p>
        </div>
        {travel.canCancel && <button type="button" className="map__jump" disabled={cancel.pending} onClick={() => cancel.run({
          action: "CancelTravel", payload: { requestId: travel.operation.requestId, expectedSession: travel.operation.session },
        })}>{cancel.pending ? "Cancelling\u2026" : "Cancel teleport"}</button>}
        {cancel.error && <p className="map__teleportsub" role="alert">{cancel.error}</p>}
      </div>
    </div>
  );
}
