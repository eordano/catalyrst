import "./ldrunserverpage.css";

export type LdRunServerPageProps = {
  setupHref: string;
  serverHref: string;
};

const SHAPES = [
  {
    name: "Content node",
    tagline: "Mirror and serve world content.",
    detail:
      "Content server, sync and postgres behind a plain LAN edge. No public exposure required \u{2014} a homelab box or the smallest VPS tier works.",
    sizing: "2 vCPU \u{B7} 8 GB RAM to start \u{2014} lower the sync memory caps on small boxes.",
  },
  {
    name: "Full realm",
    tagline: "A public realm people can walk into.",
    detail:
      "Adds voice and comms (LiveKit, archipelago), the explore/create/social/data services, world storage and profile images behind a real TLS edge.",
    sizing: "4 vCPU \u{B7} 16 GB RAM \u{2014} the shipped memory caps assume this class.",
  },
  {
    name: "Public gateway",
    tagline: "The whole surface, marketplace included.",
    detail:
      "Full realm plus the per-service gateway subdomains, asset-bundle CDN, governance, presence, telemetry and the on-chain marketplace indexer.",
    sizing:
      "4\u{2013}8 vCPU \u{B7} 16 GB RAM, plus archive-capable Ethereum and Polygon RPC endpoints for the indexer.",
  },
];

const STEPS = [
  {
    title: "Get a VPS running NixOS",
    body: "Any provider whose VMs give you root works \u{2014} use their NixOS image, or nixos-anywhere onto a rescue system. Two lessons a real install teaches: import the qemu-guest profile so the initrd has virtio drivers, and install a bootloader that covers both BIOS and UEFI \u{2014} newer VPS generations boot UEFI and silently wedge without it.",
  },
  {
    title: "Answer the setup wizard",
    body: "It generates your host configuration and the secrets files services refuse to start without. It runs entirely in your browser \u{2014} you can answer it before the server even exists.",
  },
  {
    title: "Point DNS at the machine",
    body: "Your domain, and \u{2014} for a public shape \u{2014} every certificate name the wizard lists, all resolving to the VPS address before first boot. Certificates issue themselves over HTTP-01 once they do.",
  },
  {
    title: "Boot it",
    body: "Add the module to your flake, copy the two or three generated files over, nixos-rebuild switch. Session secrets and voice keys generate themselves on first boot.",
  },
  {
    title: "Watch it come up",
    body: "/about answers with your node's manifest as soon as the content server is serving, and sync starts ingesting immediately. Nodes that also run the sites tier get the /server operations page: live per-service health with the diagnosis commands attached, rechecking itself until everything recovers.",
  },
];

const INCLUDED = [
  "A catalyst-protocol content server syncing from the peers you choose",
  "Voice and presence over your own LiveKit",
  "Places, events, marketplace and social APIs",
  "The in-browser client at /play",
  "With the sites tier: an operations page with live health and persistent configuration",
];

export default function LdRunServerPage({ setupHref, serverHref }: LdRunServerPageProps) {
  return (
    <main className="ldrun">
      <section className="ldrun-hero">
        <h1 className="ldrun-h1">Run your own server</h1>
        <p className="ldrun-lede">
          The whole realm &#x2014; content, comms, marketplace surfaces, an in-browser client &#x2014;
          ships as one NixOS module. A single VPS runs it, and every piece of it answers to
          you.
        </p>
        <div className="ldrun-ctas">
          <a className="ldrun-btn ldrun-btn-primary" href={setupHref}>
            Build your configuration
          </a>
          <a className="ldrun-btn" href="#steps">
            What it takes
          </a>
        </div>
      </section>

      <section className="ldrun-section">
        <h2 className="ldrun-h2">Pick a shape</h2>
        <div className="ldrun-shapes">
          {SHAPES.map((s) => (
            <article key={s.name} className="ldrun-shape">
              <h3 className="ldrun-h3">{s.name}</h3>
              <p className="ldrun-tagline">{s.tagline}</p>
              <p className="ldrun-detail">{s.detail}</p>
              <p className="ldrun-sizing">{s.sizing}</p>
            </article>
          ))}
        </div>
        <p className="ldrun-note">
          Disk is the real budget line: a syncing content store grows continuously into the
          hundreds of gigabytes. Start around 250 GB for a public shape and watch the disk
          line on the operations page &#x2014; it warns before full, because a full disk takes
          postgres and everything else down with it.
        </p>
      </section>

      <section className="ldrun-section" id="steps">
        <h2 className="ldrun-h2">From zero to serving</h2>
        <ol className="ldrun-steps">
          {STEPS.map((s) => (
            <li key={s.title} className="ldrun-step">
              <h3 className="ldrun-h3">{s.title}</h3>
              <p className="ldrun-detail">{s.body}</p>
            </li>
          ))}
        </ol>
      </section>

      <section className="ldrun-section">
        <h2 className="ldrun-h2">What you end up with</h2>
        <ul className="ldrun-included">
          {INCLUDED.map((i) => (
            <li key={i}>{i}</li>
          ))}
        </ul>
        <p className="ldrun-note">
          Joining the network's official server list is a governance step, separate from
          running the software {"\u{2014}"} your node serves its realm either way.
        </p>
      </section>

      <section className="ldrun-hero ldrun-hero-tail">
        <a className="ldrun-btn ldrun-btn-primary" href={setupHref}>
          Build your configuration
        </a>
        <p className="ldrun-note">
          Already running one? <a href={serverHref}>Operations live at /server</a>.
        </p>
      </section>
    </main>
  );
}
