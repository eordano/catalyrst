import { Link } from "react-router";

import { href } from "@core/lib/router/routes";

type AdminConsole = "places" | "communities" | "whatson" | "metrics";

type Props = { current: AdminConsole };

function current(id: AdminConsole, active: AdminConsole) {
  return id === active ? ("page" as const) : undefined;
}

export default function AdminConsoleLinks({ current: active }: Props) {
  return (
    <>
      <Link prefetch="intent" to={href("/admin/places-moderation")} className="adm-pill" aria-current={current("places", active)}>
        Places
      </Link>
      <Link prefetch="intent" to={href("/admin/communities-moderation")} className="adm-pill" aria-current={current("communities", active)}>
        Communities
      </Link>
      <Link prefetch="intent" to={href("/admin/whatson-users")} className="adm-pill" aria-current={current("whatson", active)}>
        What's On
      </Link>
      <Link prefetch="intent" to={href("/admin/metrics")} className="adm-pill" aria-current={current("metrics", active)}>
        Metrics
      </Link>
    </>
  );
}
