import { useState, type ReactNode } from "react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";

import { queryClient as appQueryClient } from "../app/queryClient";

export default function OverlayQueryProvider({ children }: { children: ReactNode }) {
  const [client] = useState(
    () => new QueryClient({ defaultOptions: appQueryClient.getDefaultOptions() }),
  );
  return <QueryClientProvider client={client}>{children}</QueryClientProvider>;
}
