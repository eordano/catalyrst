import LdRunServerPage from "@ui/landings/pages/LdRunServerPage";
import SitesChrome from "@ui/web/frames/SitesChrome";

export function meta() {
  return [
    { title: "Run your own server" },
    {
      name: "description",
      content:
        "The whole realm ships as one NixOS module. Pick a shape, answer the setup wizard, point a VPS at it.",
    },
  ];
}

export default function LandingsRunAServer() {
  return (
    <SitesChrome>
      <LdRunServerPage setupHref="/server/setup" serverHref="/server" />
    </SitesChrome>
  );
}
