import { useQuery } from "@tanstack/react-query";

import { fetchEvents, fetchEventCategories, type EventsParams } from "../catalyst/events";
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
