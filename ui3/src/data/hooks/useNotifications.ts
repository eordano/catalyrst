import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { qk, STALE } from "../queryKeys";
import {
  fetchLiveNotifications,
  markNotificationsRead,
  unreadCount,
  type Notification,
} from "../catalyst/notifications";
import { useBridgeState } from "../../overlay/bridge";

function authHeaders(): Record<string, string> | undefined {
  if (typeof window === "undefined") return undefined;
  return window.__DCL_AUTH_HEADERS__ || undefined;
}

export function useNotifications() {
  const identity = useBridgeState((s) => s.identity);
  const address = identity?.address;
  const queryClient = useQueryClient();
  const queryKey = qk.notifications(address);

  const query = useQuery({
    queryKey,
    queryFn: ({ signal }) =>
      fetchLiveNotifications({ signal, headers: authHeaders(), address }),
    staleTime: STALE.notifications,
    refetchOnMount: "always",
    retry: false,
  });

  const notifications = query.isSuccess
    ? query.data
    : query.isError
      ? []
      : undefined;

  const markReadMutation = useMutation({
    mutationFn: (ids: string[]) => markNotificationsRead(ids, { headers: authHeaders() }),
    onSuccess: (_res, ids) => {
      const idSet = new Set(ids);
      queryClient.setQueryData<Notification[]>(queryKey, (prev) =>
        prev?.map((n) => (idSet.has(n.id) ? { ...n, read: true } : n)),
      );
    },
  });

  const markRead = (ids: string[]) => {
    if (ids.length > 0) markReadMutation.mutate(ids);
  };

  const markAllRead = () => {
    const ids = (notifications ?? []).filter((n) => !n.read).map((n) => n.id);
    if (ids.length > 0) markReadMutation.mutate(ids);
  };

  return {
    notifications,
    address,
    unread: unreadCount(notifications),
    isLoading: query.isPending,
    isError: query.isError,
    isFetching: query.isFetching,
    refetch: query.refetch,
    markRead,
    markAllRead,
  };
}
