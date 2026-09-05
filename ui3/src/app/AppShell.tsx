import { RouterProvider } from "react-router";

import { router } from "./router";
import ErrorBoundary from "./ErrorBoundary";

export default function AppShell() {
  return (
    <ErrorBoundary>
      <RouterProvider router={router} />
    </ErrorBoundary>
  );
}
