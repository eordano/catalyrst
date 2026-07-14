import { useCallback, useState } from "react";
import {
  useMutation,
  useQuery,
  useQueryClient,
  type QueryClient,
} from "@tanstack/react-query";

import {
  fetchEvents,
  fetchEventCategories,
  fetchEventAttendees,
  isAttending,
  setEventAttendance,
  type DclEvent,
  type EventAttendee,
  type EventsParams,
} from "../catalyst/events";
import { qk, STALE } from "../queryKeys";

export function useEvents(params: EventsParams = {}) {
  return useQuery({
    queryKey: qk.events(params),
    queryFn: ({ signal }) => fetchEvents(params, { signal }),
    staleTime: STALE.events,
  });
}

export function useEventCategories() {
  return useQuery({
    queryKey: qk.eventCategories(),
    queryFn: ({ signal }) => fetchEventCategories({ signal }),
    staleTime: STALE.eventCategories,
  });
}

export const INTERESTED_ERROR_MESSAGE =
  "There was an error changing your interest on the event. Please try again.";

type EventsPage = { data: DclEvent[]; total: number };

function bumpEventTotals(qc: QueryClient, eventId: string, delta: number) {
  qc.setQueriesData<EventsPage | undefined>({ queryKey: ["events"] }, (old) => {
    if (!old) return old;
    return {
      ...old,
      data: old.data.map((e) =>
        e.id === eventId
          ? { ...e, total_attendees: Math.max(0, e.total_attendees + delta) }
          : e,
      ),
    };
  });
}

export function useEventAttendance(eventId: string | null, address: string | null) {
  const qc = useQueryClient();
  const key = qk.eventAttendees(eventId);
  const listQ = useQuery({
    queryKey: key,
    queryFn: ({ signal }) => fetchEventAttendees(eventId ?? "", { signal }),
    enabled: !!eventId,
    staleTime: STALE.eventAttendees,
  });
  const [failed, setFailed] = useState(false);

  const attendees = listQ.data;
  const attending = isAttending(attendees ?? [], address);

  const mutation = useMutation({
    mutationFn: (next: boolean) => setEventAttendance(eventId ?? "", next),
    onMutate: async (next) => {
      setFailed(false);
      await qc.cancelQueries({ queryKey: key });
      await qc.cancelQueries({ queryKey: ["events"] });
      const prev = qc.getQueryData<EventAttendee[]>(key);
      const prevEvents = qc.getQueriesData<EventsPage | undefined>({
        queryKey: ["events"],
      });
      const self = (address ?? "").toLowerCase();
      qc.setQueryData<EventAttendee[]>(key, (old = []) =>
        next
          ? [
              ...old,
              {
                event_id: eventId ?? "",
                user: self,
                user_name: null,
                created_at: new Date().toISOString(),
              },
            ]
          : old.filter((a) => a.user.toLowerCase() !== self),
      );
      if (eventId) bumpEventTotals(qc, eventId, next ? 1 : -1);
      return { prev, prevEvents };
    },
    onError: (_err, _next, ctx) => {
      qc.setQueryData(key, ctx?.prev ?? []);
      for (const [k, data] of ctx?.prevEvents ?? []) qc.setQueryData(k, data);
      setFailed(true);
    },
    onSuccess: (list) => {
      qc.setQueryData(key, list);
    },
    onSettled: () => {
      qc.invalidateQueries({ queryKey: ["events"] });
    },
  });

  const { mutate, isPending } = mutation;
  const toggle = useCallback(() => {
    if (!eventId || !address || isPending) return;
    mutate(!attending);
  }, [eventId, address, attending, isPending, mutate]);

  return {
    attending,
    count: attendees?.length,
    toggle,
    pending: isPending,
    error: failed ? INTERESTED_ERROR_MESSAGE : null,
  };
}
