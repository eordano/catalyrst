import { plural } from "../fmt";
import { FD_ROLES, type FdRoleDoor } from "../components/FdDoors";
import FdRoleCard, {
  type FdRoleId,
  type FdRoleStateLine,
} from "../components/FdRoleCard";
import FdSection, { FdPageHead } from "../components/FdSection";
import "../components/fdrolecard.css";
import "./foundryhome.css";

export type FoundryHomeProps = {
  onChoose?: (id: FdRoleId) => void;
  doors?: readonly FdRoleDoor[];
  /** One live reading per door, from the program database. Absent (no database
   *  on this deployment) renders the doors static. */
  doorState?: Partial<Record<FdRoleId, FdRoleStateLine>>;
};

export default function FoundryHome(props: FoundryHomeProps) {
  const { onChoose, doors = FD_ROLES, doorState } = props;
  return (
    <div className="fd-page fd-stack fd-home">
      <FdPageHead
        display
        title="The Foundry"
        intro="An open build bench for Decentraland games."
      />

      <FdSection
        title="Doors"
        badge={<span className="fd-chip">{plural(doors.length, "door")}</span>}
      >
        <ul className="fd-board fd-home__doors">
          {doors.map((door) => (
            <FdRoleCard
              key={door.id}
              role={door}
              stateLine={doorState?.[door.id] ?? null}
              onChoose={onChoose}
            />
          ))}
        </ul>
      </FdSection>
    </div>
  );
}
