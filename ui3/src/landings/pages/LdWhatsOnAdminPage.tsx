import type { ReactNode } from "react";

import "./ldwhatsonadminpage.css";

type LdWhatsOnAdminPageProps = {
  children?: ReactNode;
};

export default function LdWhatsOnAdminPage({
  children = null,
}: LdWhatsOnAdminPageProps) {
  return <main className="whatson-admin-route">{children}</main>;
}
