import { useState } from "react";
import { SitesChromeMaybe } from "../frames/SitesChrome";
import { CheckFill } from "../../atoms/icons";
import "./stsocialcommunitydetail.css";

export type Community = {
  id: string;
  name: string;
  description: string;
  ownerAddress: string;
  ownerName?: string | null;
  ownerProfilePicture?: string;
  privacy: "public" | "private";
  membersCount: number;
  thumbnail?: string;
  role?: string;
};

export type Member = {
  memberAddress: string;
  name: string;
  role: string;
  hasClaimedName: boolean;
  profilePictureUrl?: string;
};

export type CommunityEvent = {
  id: string;
  name: string;
  image?: string;
  creatorName: string;
  creatorAvatarUrl?: string;
  timeLabel: string;
};

const T = {
  decentraland_community: "Decentraland Community",
  members: "Members",
  by: "By",
  unknown: "Unknown",
  joined: "JOINED",
  sign_in_to_join: "SIGN IN TO JOIN",
  request_to_join: "REQUEST TO JOIN",
  cancel_request: "CANCEL REQUEST",
  jump_in: "JUMP IN",
  join: "JOIN",
  loading: "Loading...",
  not_found: "Community not found",
  not_found_description: "The community you are looking for does not exist.",
  upcoming_events: "UPCOMING EVENTS",
  no_upcoming_events: "No Upcoming Events",
  members_title: "MEMBERS",
  no_members_found: "No members found",
  private_title: "This is a Private Community",
  private_description: "You must be a member of this Community to see its profile.",
  tab_members: "Members",
  tab_events: "Upcoming Events",
};

const PrivacyGlyph = () => (
  <svg width="13.18" height="13.18" viewBox="0 0 14 14" aria-hidden="true">
    <path
      d="M6.59004 0C2.95629 0 0 2.95629 0 6.59004C0 10.2238 2.95629 13.1801 6.59004 13.1801C10.2238 13.1801 13.1801 10.2238 13.1801 6.59004C13.1801 2.95629 10.2238 0 6.59004 0ZM1.31801 6.59004C1.31801 5.99759 1.42081 5.42887 1.60204 4.89706L2.63602 5.93104L3.95402 7.24904V8.56705L5.27203 9.88506L5.93104 10.5441V11.8166C3.33522 11.4904 1.31801 9.2735 1.31801 6.59004ZM10.7615 9.80136C10.3312 9.45473 9.67879 9.22606 9.22606 9.22606V8.56705C9.22606 8.21749 9.08719 7.88225 8.84002 7.63508C8.59284 7.3879 8.2576 7.24904 7.90805 7.24904H5.27203V5.27203C5.62159 5.27203 5.95683 5.13317 6.204 4.886C6.45118 4.63882 6.59004 4.30358 6.59004 3.95402V3.29502H7.24904C7.5986 3.29502 7.93384 3.15616 8.18102 2.90898C8.42819 2.66181 8.56705 2.32657 8.56705 1.97701V1.70616C10.4966 2.48972 11.8621 4.38238 11.8621 6.59004C11.862 7.75298 11.4748 8.8828 10.7615 9.80136Z"
      fill="#CFCDD4"
    />
  </svg>
);

const GroupsGlyph = () => (
  <svg className="stscd__imagefallback" width="96" height="96" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d="M16 11c1.66 0 2.99-1.34 2.99-3S17.66 5 16 5c-1.66 0-3 1.34-3 3s1.34 3 3 3zm-8 0c1.66 0 2.99-1.34 2.99-3S9.66 5 8 5C6.34 5 5 6.34 5 8s1.34 3 3 3zm0 2c-2.33 0-7 1.17-7 3.5V19h14v-2.5c0-2.33-4.67-3.5-7-3.5zm8 0c-.29 0-.62.02-.97.05 1.16.84 1.97 1.97 1.97 3.45V19h6v-2.5c0-2.33-4.67-3.5-7-3.5z" />
  </svg>
);

const JumpInGlyph = () => (
  <svg width="18" height="18" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d="M10 6 8.59 7.41 13.17 12l-4.58 4.59L10 18l6-6z" />
  </svg>
);

const ClaimedGlyph = () => (
  <svg className="stscd__claimed" viewBox="0 0 24 24" aria-hidden="true" aria-label="Claimed name badge">
    <path
      d="M12 1 9.5 3.5 6 3 5.2 6.5 2 8l1.6 3.2L2 14.4 5.2 16 6 19.5l3.5-.5L12 22l2.5-3 3.5.5.8-3.5L22 14.4l-1.6-3.2L22 8l-3.2-1.5L18 3l-3.5.5z"
      fill="var(--brand)"
    />
    <path d="m8.5 12 2.4 2.4 4.6-4.8" stroke="#fff" strokeWidth="1.8" strokeLinecap="round" strokeLinejoin="round" fill="none" />
  </svg>
);

const ClockGlyph = () => (
  <svg viewBox="0 0 24 24" fill="var(--stscd-white)" aria-hidden="true">
    <path d="M11.99 2C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8zM12.5 7H11v6l5.25 3.15.75-1.23-4.5-2.67z" />
  </svg>
);

const ErrorOutlineGlyph = () => (
  <svg className="stscd__notfoundicon" width="72" height="72" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d="M11 15h2v2h-2zm0-8h2v6h-2zm.99-5C6.47 2 2 6.48 2 12s4.47 10 9.99 10C17.52 22 22 17.52 22 12S17.52 2 11.99 2zM12 20c-4.42 0-8-3.58-8-8s3.58-8 8-8 8 3.58 8 8-3.58 8-8 8z" />
  </svg>
);

const LockGlyph = () => (
  <svg width="100" height="100" viewBox="0 0 100 100" fill="none" aria-hidden="true">
    <rect x="28" y="38.0001" width="47" height="43" rx="8" fill="#FFA25A" />
    <path
      d="M36.7112 32.7483H62.9902V21.0001C62.9902 15.4773 58.513 11.0001 52.9902 11.0001H46.7112C41.1883 11.0001 36.7112 15.4773 36.7112 21.0001V32.7483Z"
      stroke="#FCFCFC"
      strokeWidth="4"
    />
    <rect x="22" y="33" width="55" height="50" rx="12" stroke="#FCFCFC" strokeWidth="4" />
    <path
      d="M49.3975 48.7773C52.9007 48.7773 55.7412 51.6178 55.7412 55.1211C55.7412 57.6765 54.2291 59.8771 52.0518 60.8818C52.0927 61.0695 52.1162 61.2639 52.1162 61.4639V67.8076C52.1161 69.3089 50.8988 70.5264 49.3975 70.5264C47.8962 70.5262 46.6798 69.3089 46.6797 67.8076V61.4639C46.6797 61.264 46.7022 61.0694 46.7432 60.8818C44.5662 59.877 43.0547 57.6762 43.0547 55.1211C43.0547 51.618 45.8944 48.7776 49.3975 48.7773Z"
      fill="white"
    />
  </svg>
);

const EmptyEventsGlyph = () => (
  <svg className="stscd__emptyicon" width="64" height="64" viewBox="0 0 24 24" fill="currentColor" aria-hidden="true">
    <path d="M19 4h-1V2h-2v2H8V2H6v2H5c-1.11 0-1.99.9-1.99 2L3 20c0 1.1.89 2 2 2h14c1.1 0 2-.9 2-2V6c0-1.1-.9-2-2-2zm0 16H5V10h14v10zm0-12H5V6h14v2z" />
  </svg>
);

function avatarBg(seed: string): string {
  let h = 0;
  for (let i = 0; i < seed.length; i++) h = (h * 31 + seed.charCodeAt(i)) >>> 0;
  return `linear-gradient(135deg, hsl(${h % 360} 60% 55%), hsl(${(h + 60) % 360} 55% 42%))`;
}

const compact = (n: number): string =>
  new Intl.NumberFormat("en-US", { notation: "compact", compactDisplay: "short" }).format(n);

type CommunityInfoProps = {
  community: Community;
  isLoggedIn: boolean;
  isMember: boolean;
  hasPendingRequest: boolean;
  canViewContent: boolean;
  showJumpIn: boolean;
};

function CommunityInfo({ community, isLoggedIn, isMember, hasPendingRequest, canViewContent, showJumpIn }: CommunityInfoProps) {
  const isPrivate = community.privacy === "private";
  const renderCta = () => {
    if (isMember) {
      return (
        <button className="stscd__cta stscd__cta--joined" disabled>
          <CheckFill className="stscd__ctacheck" />
          {T.joined}
        </button>
      );
    }
    if (!isLoggedIn) {
      return <button className="stscd__cta stscd__cta--primary">{T.sign_in_to_join}</button>;
    }
    if (isPrivate) {
      return (
        <button className="stscd__cta stscd__cta--secondary">
          {hasPendingRequest ? T.cancel_request : T.request_to_join}
        </button>
      );
    }
    return <button className="stscd__cta stscd__cta--primary">{T.join}</button>;
  };

  return (
    <section className="stscd__info">
      <div className="stscd__toprow">
        <div className="stscd__image">
          {community.thumbnail ? (
            <img src={community.thumbnail} alt={community.name} />
          ) : (
            <GroupsGlyph />
          )}
        </div>
        <div className="stscd__details">
          <div className="stscd__titlecontainer">
            <div className="stscd__titleheader">
              <p className="stscd__label">{T.decentraland_community}</p>
              <h1 className="stscd__title">{community.name}</h1>
              <div className="stscd__privrow">
                <span className="stscd__privbadge">
                  <span className="stscd__privicon">
                    <PrivacyGlyph />
                  </span>
                  <span className="stscd__privbadgetext">{community.privacy}</span>
                </span>
                <span className="stscd__privdivider" />
                <span className="stscd__privmembers">
                  {compact(community.membersCount)} {T.members}
                </span>
              </div>
            </div>
            <div className="stscd__ownerrow">
              <span
                className="stscd__owneravatar"
                style={
                  community.ownerProfilePicture
                    ? { backgroundImage: `url(${community.ownerProfilePicture})` }
                    : { background: avatarBg(community.ownerAddress) }
                }
              />
              <span className="stscd__ownertext">
                {T.by} <span className="stscd__ownername">{community.ownerName ?? T.unknown}</span>
              </span>
            </div>
            <div className="stscd__actions">
              {renderCta()}
              {showJumpIn && (
                <button className="stscd__jumpin">
                  {T.jump_in}
                  <JumpInGlyph />
                </button>
              )}
            </div>
          </div>
          {canViewContent && <p className="stscd__description">{community.description}</p>}
        </div>
      </div>
    </section>
  );
}

function MemberCard({ member }: { member: Member }) {
  return (
    <div className="stscd__member">
      <span
        className="stscd__memberavatar"
        style={
          member.profilePictureUrl
            ? { backgroundImage: `url(${member.profilePictureUrl})` }
            : { background: avatarBg(member.memberAddress) }
        }
      />
      <div className="stscd__memberinfo">
        <div className="stscd__membernamerow">
          <span className="stscd__membername">{member.name}</span>
          {member.hasClaimedName && <ClaimedGlyph />}
        </div>
        <span className="stscd__memberrole">{member.role}</span>
      </div>
    </div>
  );
}

type MembersListProps = {
  members: Member[];
  isLoading: boolean;
  total?: number;
  hideTitle?: boolean;
  showCount?: boolean;
};

function MembersList({ members, isLoading, total, hideTitle, showCount = true }: MembersListProps) {
  const count = typeof total === "number" ? total : members.length;
  const title = showCount ? `${T.members_title} (${count})` : T.members_title;
  return (
    <div className="stscd__section">
      {!hideTitle && <h2 className="stscd__sectiontitle">{title}</h2>}
      {isLoading ? (
        <div className="stscd__loader" style={{ minHeight: 200 }}>
          <div className="stscd__spinner" />
        </div>
      ) : members.length === 0 ? (
        <div className="stscd__empty">
          <span className="stscd__emptytext">{T.no_members_found}</span>
        </div>
      ) : (
        <div className="stscd__memberlist" role="region" aria-label={T.members_title} tabIndex={0}>
          {members.map((m) => (
            <MemberCard key={m.memberAddress} member={m} />
          ))}
        </div>
      )}
    </div>
  );
}

function EventCard({ event }: { event: CommunityEvent }) {
  return (
    <button type="button" className="stscd__event">
      {event.image && (
        <div className="stscd__eventthumb">
          <img src={event.image} alt={event.name} loading="lazy" />
        </div>
      )}
      <div className="stscd__eventbody">
        <div className="stscd__eventtop">
          <p className="stscd__eventtitle">{event.name}</p>
          <div className="stscd__eventcreator">
            {event.creatorAvatarUrl ? (
              <img src={event.creatorAvatarUrl} alt={event.creatorName} />
            ) : (
              <span className="stscd__eventcreatorfallback" />
            )}
            <span className="stscd__eventcreatorname">
              by <b>{event.creatorName}</b>
            </span>
          </div>
        </div>
        <span className="stscd__eventtime">
          <ClockGlyph />
          <span className="stscd__eventtimelabel">{event.timeLabel}</span>
        </span>
      </div>
    </button>
  );
}

type EventsListProps = {
  events: CommunityEvent[];
  isLoading: boolean;
  hideTitle?: boolean;
};

function EventsList({ events, isLoading, hideTitle }: EventsListProps) {
  return (
    <div className="stscd__section">
      {!hideTitle && <h2 className="stscd__sectiontitle">{T.upcoming_events}</h2>}
      {isLoading ? (
        <div className="stscd__loader" style={{ minHeight: 200 }}>
          <div className="stscd__spinner" />
        </div>
      ) : events.length === 0 ? (
        <div className="stscd__empty stscd__empty--events">
          <EmptyEventsGlyph />
          <span className="stscd__emptytext--events">{T.no_upcoming_events}</span>
        </div>
      ) : (
        <div className="stscd__eventsgrid">
          {events.map((e) => (
            <EventCard key={e.id} event={e} />
          ))}
        </div>
      )}
    </div>
  );
}

function PrivateMessage() {
  return (
    <div className="stscd__private">
      <LockGlyph />
      <div className="stscd__privatecontent">
        <h2 className="stscd__privatetitle">{T.private_title}</h2>
        <p className="stscd__privatetext">{T.private_description}</p>
      </div>
    </div>
  );
}

const EMPTY_COMMUNITY: Community = {
  id: "",
  name: "",
  description: "",
  ownerAddress: "",
  privacy: "public",
  membersCount: 0,
};

type StSocialCommunityDetailProps = {
  chrome?: boolean;
  community?: Community;
  members?: Member[];
  events?: CommunityEvent[];
  membersTotal?: number;
  isLoggedIn?: boolean;
  isMember?: boolean;
  hasPendingRequest?: boolean;
  isLoadingMembers?: boolean;
  isLoadingEvents?: boolean;
  state?: "default" | "loading" | "notFound";
  mobile?: boolean;
};

export default function StSocialCommunityDetail({
  chrome = true,
  community = EMPTY_COMMUNITY,
  members = [],
  events = [],
  membersTotal,
  isLoggedIn = false,
  isMember = false,
  hasPendingRequest = false,
  isLoadingMembers = false,
  isLoadingEvents = false,
  state = "default",
  mobile = false,
}: StSocialCommunityDetailProps) {
  const [activeTab, setActiveTab] = useState<"members" | "events">("members");

  if (state === "loading") {
    return (
      <SitesChromeMaybe chrome={chrome} active="play">
        <div className="stscd">
          <div className="stscd__content">
            <div className="stscd__loader">
              <div className="stscd__spinner" />
            </div>
          </div>
        </div>
      </SitesChromeMaybe>
    );
  }

  if (state === "notFound") {
    return (
      <SitesChromeMaybe chrome={chrome} active="play">
        <div className="stscd">
          <div className="stscd__content">
            <div className="stscd__notfound">
              <ErrorOutlineGlyph />
              <h1 className="stscd__notfoundtitle">{T.not_found}</h1>
              <p className="stscd__notfounddesc">{T.not_found_description}</p>
            </div>
          </div>
        </div>
      </SitesChromeMaybe>
    );
  }

  const isPrivate = community.privacy === "private";
  const canViewContent = isMember || !isPrivate;
  const showJumpIn = isLoggedIn && !mobile && (isMember || hasPendingRequest);

  return (
    <SitesChromeMaybe chrome={chrome} active="play">
      <div className="stscd">
        <div className="stscd__content">
          <CommunityInfo
            community={community}
            isLoggedIn={isLoggedIn}
            isMember={isMember}
            hasPendingRequest={hasPendingRequest}
            canViewContent={canViewContent}
            showJumpIn={showJumpIn}
          />

          {!canViewContent && <PrivateMessage />}

          {canViewContent &&
            (mobile ? (
              <div className="stscd__bottom">
                <div className="stscd__tabs">
                  <button
                    type="button"
                    className={"stscd__tab" + (activeTab === "members" ? " is-active" : "")}
                    onClick={() => setActiveTab("members")}
                  >
                    {T.tab_members}
                  </button>
                  <button
                    type="button"
                    className={"stscd__tab" + (activeTab === "events" ? " is-active" : "")}
                    onClick={() => setActiveTab("events")}
                  >
                    {T.tab_events}
                  </button>
                </div>
                {activeTab === "members" ? (
                  <div className="stscd__members-col">
                    <MembersList
                      members={members}
                      isLoading={isLoadingMembers}
                      total={membersTotal ?? community.membersCount}
                      hideTitle
                      showCount={false}
                    />
                  </div>
                ) : (
                  <div className="stscd__events-col">
                    <EventsList events={events} isLoading={isLoadingEvents} hideTitle />
                  </div>
                )}
              </div>
            ) : (
              <div className="stscd__bottom">
                <div className="stscd__members-col">
                  <MembersList
                    members={members}
                    isLoading={isLoadingMembers}
                    total={membersTotal ?? community.membersCount}
                  />
                </div>
                <div className="stscd__events-col">
                  <EventsList events={events} isLoading={isLoadingEvents} />
                </div>
              </div>
            ))}
        </div>
      </div>
    </SitesChromeMaybe>
  );
}
