import { createContext, useContext } from "react";
import { useNavigate } from "react-router";
export type EntryDestination =
  | { kind: "world"; realm: string; parcel?: [number, number] }
  | { kind: "parcel"; x: number; y: number }
  | null;

export const WorldEntryContext = createContext<{
  pending: boolean;
  destination?: EntryDestination;
  enter(destination: EntryDestination): void;
} | null>(null);

export function useWorldEntry() {
  const entry = useContext(WorldEntryContext);
  const navigate = useNavigate();
  return entry && { ...entry, enter(destination: EntryDestination) {
    navigate("/");
    entry.enter(destination);
  } };
}
