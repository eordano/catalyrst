import { useCallback, useEffect, useRef, useState } from "react";
import { editorPageTransition, initialEditorPage, type EditorPageCommand, type EditorPageEvent, type EditorPageRequest } from "../page-machine";

export function useEditorPageMachine(scope: string) {
  const current = useRef(initialEditorPage());
  const sequence = useRef(0);
  const controllers = useRef(new Map<EditorPageRequest, AbortController>());
  const [state, setState] = useState(current.current);
  const send = useCallback((event: EditorPageEvent) => {
    const previous = current.current.pending;
    current.current = editorPageTransition(current.current, event);
    if (previous && previous !== current.current.pending) {
      if (event.type !== "finished") controllers.current.get(previous)?.abort();
      controllers.current.delete(previous);
    }
    setState(current.current);
    return current.current;
  }, []);
  useEffect(() => {
    send({ type: "reset" });
    return () => {
      for (const controller of controllers.current.values()) controller.abort();
      controllers.current.clear();
      current.current = editorPageTransition(current.current, { type: "reset" });
    };
  }, [scope, send]);
  return {
    state, current, send,
    begin(command: EditorPageCommand, allowed: boolean): EditorPageRequest | null {
      const request = { generation: current.current.generation, id: ++sequence.current, command, revision: current.current.revision };
      if (send({ type: "request", request, allowed }).pending !== request) return null;
      controllers.current.set(request, new AbortController());
      return request;
    },
    signal: (request: EditorPageRequest) => controllers.current.get(request)?.signal ?? AbortSignal.abort(),
    accepts: (request: EditorPageRequest) => current.current.pending === request,
  };
}
