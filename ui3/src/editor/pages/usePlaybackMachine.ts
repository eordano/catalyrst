import { useCallback, useEffect, useRef, useState } from "react";
import { initialPlayback, playbackTransition, type PlaybackCommand, type PlaybackEvent, type PlaybackRequest } from "../playback-machine";

export function usePlaybackMachine() {
  const current = useRef(initialPlayback());
  const sequence = useRef(0);
  const [state, setState] = useState(current.current);
  const send = useCallback((event: PlaybackEvent) => {
    current.current = playbackTransition(current.current, event);
    setState(current.current);
    return current.current;
  }, []);
  useEffect(() => () => { current.current = playbackTransition(current.current, { type: "reset" }); }, []);
  return {
    state,
    current,
    send,
    begin(command: PlaybackCommand, ready: boolean): PlaybackRequest | null {
      const request = { generation: current.current.generation, id: ++sequence.current, command };
      return send({ type: "request", request, ready }).pending === request ? request : null;
    },
    accepts: (request: PlaybackRequest) => current.current.pending === request,
  };
}
