import FdAvatarCrowd, {
  type FdAvatarCrowdViewer,
} from "../components/FdAvatarCrowd";
import FdRoleCard, {
  type FdRoleCardVM,
  type FdRoleId,
} from "../components/FdRoleCard";
import FdSection, { FdPageHead } from "../components/FdSection";
import "./fdselect.css";

export type { FdRoleId };

export type FdRoleDestination = "play" | "editor" | "console";

export type FdRoleDoor = FdRoleCardVM & { destinationId: FdRoleDestination };

/** The same sentence under all three doors. Repetition is the argument. */
export const FD_SAME_LINE = "Same Decentraland as the other two doors.";

export const FD_ROLES: readonly FdRoleDoor[] = [
  {
    id: "start",
    role: "Start",
    who: "the player's door",
    title: "Show up and play.",
    body:
      "Walk into a live world and play something a person made, in a room with other people in it. No install, no reading, no application form. This is where your people gather; the fastest way to find them is to be there.",
    blurLine:
      "Not a class — a starting point. Everyone here plays, including the people who built the place you land in.",
    destination: "Opens the games in the Foundry.",
    destinationId: "play",
    href: "/foundry/play",
    cta: "Start playing",
  },
  {
    id: "create",
    role: "Create",
    who: "the builder's door",
    title: "Build something your friends will show up for.",
    body:
      "Open the scene editor and make a room, a game, a stage worth standing in. Building is play too — it is the same night out, held from the other side. Bring your crew and make them a reason to come back.",
    blurLine:
      "Not a class — a starting point. Every builder here began as somebody who showed up to play.",
    destination: "Opens the scene editor on a new scene.",
    destinationId: "editor",
    href: "/creator-hub/scene-editor?new=1&from=foundry",
    cta: "Start building",
  },
  {
    id: "admin",
    role: "Admin",
    who: "the operator's door",
    title: "See how the program runs.",
    body:
      "Open the Foundry console: the gate checks each game passes, the bot-bench runs, the trajectory log behind every run, and what the copilot's tokens cost. It is the program's read-out on itself — the controls for hosting your own world are not built yet.",
    blurLine:
      "Not a class — a starting point. The console is where the program shows its work, not a hosting panel.",
    destination: "Opens the Foundry console.",
    destinationId: "console",
    href: "/foundry/console",
    cta: "Open the console",
  },
];

const BEATS: readonly { id: string; step: string; title: string; body: string }[] = [
  {
    id: "play",
    step: "01",
    title: "Play",
    body:
      "Nobody arrives for a platform. You arrive because something looked fun. That has always been the only entry requirement.",
  },
  {
    id: "social",
    step: "02",
    title: "Social response",
    body:
      "You stay for who is there. A game is a reason to be in a room with people, better still the moment a friend of yours walks in.",
  },
  {
    id: "rights",
    step: "03",
    title: "Rights",
    body:
      "You keep what you make. Names, land, scenes, wearables — held by you, which is what turns a crowd into a society.",
  },
];

export type FdSelectPageProps = {
  viewer?: FdAvatarCrowdViewer | null;
  onChoose?: (id: FdRoleId) => void;
  roles?: readonly FdRoleDoor[];
};

export default function FdSelectPage({
  viewer = null,
  onChoose,
  roles = FD_ROLES,
}: FdSelectPageProps) {
  return (
    <div className="fd-page fd-stack fd-select">
      <FdPageHead
        eyebrow="Start here"
        title="Three doors. One society."
        intro="Pick the door that sounds like you today. All three open on the same Decentraland — the same worlds, the same crowd, the same night. A door is where you come in, not who you are."
      />

      <section className="fd-select__crowd" aria-labelledby="fd-select-crowd">
        <FdAvatarCrowd
          viewer={viewer}
          caption={
            viewer
              ? "Your avatar, next to Decentraland's base avatars — rendered live from the content server."
              : "Decentraland's base avatars, rendered live from the content server. Sign in and the first one becomes yours."
          }
        />
        <div className="fd-select__crowdcopy">
          <h2 className="fd-select__crowdtitle" id="fd-select-crowd">
            This is where your people gather.
          </h2>
          <p className="fd-select__crowdbody">
            Decentraland is not a product you use on your own. It is a place built
            to be shared, and it is better the moment someone you know is standing
            in it with you. Bring your crew, or find one when you land — either way
            you are walking into a room, not opening an app.
          </p>
          <p className="fd-select__crowdnote">
            We do not print a friend count here. We would have to make one up, and
            the whole point of this place is that the people in it are real.
          </p>
        </div>
      </section>

      <FdSection
        title="Pick a door"
        sub="Three doors, listed in the order most people meet them. None of them is a rank, a tier or an application — they differ only in what happens in the first ten seconds after you click."
      >
        <ul className="fd-select__doors">
          {roles.map((role) => (
            <FdRoleCard
              key={role.id}
              role={role}
              sameLine={FD_SAME_LINE}
              onChoose={onChoose}
            />
          ))}
        </ul>
      </FdSection>

      <section className="fd-select__band" aria-labelledby="fd-select-band">
        <p className="fd-select__bandkicker">One society</p>
        <h2 className="fd-select__bandtitle" id="fd-select-band">
          The roles blur on purpose.
        </h2>
        <p className="fd-select__bandlede">
          Player, builder, host: these are three verbs, not three castes. Today's
          player is next month's builder and next year's host, usually because a
          friend asked. Nothing locks behind the door you picked, and you can walk
          back out and try another one whenever you like.
        </p>
        <ol className="fd-select__beats">
          {BEATS.map((b) => (
            <li className="fd-select__beat" key={b.id}>
              <span className="fd-select__beatstep" aria-hidden="true">
                {b.step}
              </span>
              <h3 className="fd-select__beattitle">{b.title}</h3>
              <p className="fd-select__beatbody">{b.body}</p>
            </li>
          ))}
        </ol>
        <p className="fd-select__bandfoot">
          Build together. Find your people. Keep what you build. Three doors get you
          to the same place — the only question this page asks is which ten seconds
          you want first.
        </p>
      </section>

      <p className="fd-datanote" role="note">
        What is real here: the avatars above are Decentraland avatars, drawn by the
        same previewer the backpack uses from wearables served by the content
        server — the base set every new account is handed, or your own profile when
        you are signed in. No friend counts, no "players online" figure and no
        stand-in art: this page has no such data, so it shows none.
      </p>
    </div>
  );
}
