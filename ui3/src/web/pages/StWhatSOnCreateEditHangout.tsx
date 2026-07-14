import type { ReactNode } from "react";
import { useState } from "react";
import { SitesChromeMaybe, type SitesNavId } from "../frames/SitesChrome";
import "./stwhatsoncreateedithangout.css";

const COPY = {
  title: "Submit a Hangout",
  editTitle: "Edit Hangout",
  back: "Go back",
  selectCover: "Select a Hangout Cover",
  choosePicture: "Choose a Picture",
  dropHint: "from your gallery or drop it here",
  recommendedSize: "(recommended size: 1340 \u{D7} 670)",
  imageHelper:
    "Upload a PNG, JPG, or GIF (1340\u{D7}670px, max 500kb). Center visuals and text to fit all screen resolutions.",
  changeImage: "Change Image",
  addVerticalCover: "Add Vertical Cover",
  recommendedParenthetical: "(recommended)",
  eventName: "Hangout Name",
  namePlaceholder: "Be as descriptive as you can",
  eventDescription: "Hangout Description",
  descriptionPlaceholder: "Be as descriptive as you can",
  eventDetails: "Hangout Details",
  date: "Date",
  start: "Start",
  duration: "Duration",
  repeatEvent: "Repeat Hangout",
  recurrence: "Recurrence",
  repeatUntil: "Ends",
  upcomingDatesLabel: "Upcoming dates",
  upcomingDatesEmpty: "No upcoming dates for this recurrence",
  location: "Location",
  locationType: "Location Type",
  land: "Land",
  world: "World",
  latitude: "Latitude (x)",
  altitude: "Altitude (y)",
  worldPlaceholder: "Select a world",
  community: "Community",
  communityNone: "None",
  emailLabel: "Email (Optional)",
  emailPlaceholder: "hello@example.com",
  reviewNotice:
    "The hangout submission will be reviewed by our team, you'll be notified by email",
  preview: "Preview",
  cancel: "Cancel",
  submit: "Submit Hangout",
  saveChanges: "Save Changes",
  delete: "Delete",
  successMessage:
    'Your hangout was created successfully. You\u{2019}ll be notified once it\u{2019}s approved. Track its status in "My Hangouts"',
  editSuccessMessage: "Your hangout was updated successfully.",
  backToExplore: "Back to Explore",
  myEvents: "My Hangouts",
  signInMessage:
    "To create a hangout you will need to sign in or create an account",
  signIn: "Sign In",
  createAccount: "Create Account",
};

type Option = { value: string; label: string };

const RECURRENCE_OPTIONS: Option[] = [
  { value: "every_day", label: "Every Day" },
  { value: "every_week", label: "Every Week" },
  { value: "every_2_weeks", label: "Every 2 Weeks" },
  { value: "every_3_weeks", label: "Every 3 Weeks" },
  { value: "every_4_weeks", label: "Every 4 Weeks" },
  { value: "every_month", label: "Every Month" },
];

type EventData = {
  name: string;
  description: string;
  startDate: string;
  startTime: string;
  duration: string;
  repeatEnabled: boolean;
  recurrence: string;
  repeatEndDate: string;
  location: string;
  coordX: string;
  coordY: string;
  world: string;
  communityId: string;
  email: string;
  imagePreviewUrl: string | null;
  verticalImagePreviewUrl: string | null;
};

const EDIT_EVENT: EventData = {
  name: "Neon Nights \u{2014} Synthwave Live Set",
  description:
    "Join us for a two-hour synthwave & retrowave DJ set in our rooftop club. Grab a drink, dance, and meet other music lovers. Doors open at the start time \u{2014} come early for the best spot on the floor.",
  startDate: "2026-07-18",
  startTime: "20:00",
  duration: "02:00",
  repeatEnabled: true,
  recurrence: "every_week",
  repeatEndDate: "2026-09-26",
  location: "land",
  coordX: "-45",
  coordY: "120",
  world: "",
  communityId: "music",
  email: "host@neon-nights.xyz",
  imagePreviewUrl: null,
  verticalImagePreviewUrl: null,
};

const EMPTY_EVENT: EventData = {
  name: "",
  description: "",
  startDate: "",
  startTime: "",
  duration: "",
  repeatEnabled: false,
  recurrence: "every_week",
  repeatEndDate: "",
  location: "land",
  coordX: "0",
  coordY: "0",
  world: "",
  communityId: "",
  email: "",
  imagePreviewUrl: null,
  verticalImagePreviewUrl: null,
};

type Community = { id: string; name: string };

const COMMUNITIES: Community[] = [
  { id: "music", name: "Music Lovers DAO" },
  { id: "art", name: "Generative Art Collective" },
  { id: "gaming", name: "Decentraland Gamers" },
];

const WORLD_NAMES = ["my-world.dcl.eth", "neon-club.dcl.eth", "art-gallery.dcl.eth"];

function BackArrowIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
      <path
        d="M15.7 4.3a1 1 0 0 1 0 1.4L9.4 12l6.3 6.3a1 1 0 0 1-1.4 1.4l-7-7a1 1 0 0 1 0-1.4l7-7a1 1 0 0 1 1.4 0Z"
        fill="currentColor"
      />
    </svg>
  );
}

function CalendarIcon() {
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <path
        d="M17 3h-1V1h-2v2H10V1H8v2H7a4 4 0 0 0-4 4v12a4 4 0 0 0 4 4h10a4 4 0 0 0 4-4V7a4 4 0 0 0-4-4Zm2 16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V10h14v9Zm0-11H5V7a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2v1Z"
        fill="currentColor"
      />
    </svg>
  );
}

function ClockIcon() {
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <path
        d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm1 10.4V6h-2v7.4l4.5 2.6 1-1.7-3.5-1.9Z"
        fill="currentColor"
      />
    </svg>
  );
}

function CameraIcon() {
  return (
    <svg viewBox="0 0 24 24" width="64" height="64" aria-hidden="true">
      <path
        d="M9 3 7.2 5H4a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-3.2L15 3H9Zm3 5a5.5 5.5 0 1 1 0 11 5.5 5.5 0 0 1 0-11Zm0 2a3.5 3.5 0 1 0 0 7 3.5 3.5 0 0 0 0-7Z"
        fill="rgba(255,255,255,.5)"
      />
    </svg>
  );
}

function PlusIcon() {
  return (
    <svg viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <path d="M11 11V5h2v6h6v2h-6v6h-2v-6H5v-2h6Z" fill="#fcfcfc" />
    </svg>
  );
}

function InfoIcon({ size = 16 }: { size?: number }) {
  return (
    <svg viewBox="0 0 24 24" width={size} height={size} aria-hidden="true">
      <path
        d="M12 2a10 10 0 1 0 0 20 10 10 0 0 0 0-20Zm0 18a8 8 0 1 1 0-16 8 8 0 0 1 0 16Zm-1-11h2V7h-2v2Zm0 8h2v-6h-2v6Z"
        fill="currentColor"
      />
    </svg>
  );
}

function EyeIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
      <path
        d="M12 5c-5 0-9.3 3.1-11 7 1.7 3.9 6 7 11 7s9.3-3.1 11-7c-1.7-3.9-6-7-11-7Zm0 12a5 5 0 1 1 0-10 5 5 0 0 1 0 10Zm0-8a3 3 0 1 0 0 6 3 3 0 0 0 0-6Z"
        fill="currentColor"
      />
    </svg>
  );
}

function TrashIcon() {
  return (
    <svg viewBox="0 0 24 24" width="20" height="20" aria-hidden="true">
      <path
        d="M6 7h12v13a2 2 0 0 1-2 2H8a2 2 0 0 1-2-2V7Zm3 3v8h2v-8H9Zm4 0v8h2v-8h-2ZM9 4V3h6v1h5v2H4V4h5Z"
        fill="currentColor"
      />
    </svg>
  );
}

function CheckMark() {
  return (
    <svg viewBox="0 0 24 24" width="56" height="56" aria-hidden="true">
      <path
        d="M9 16.2 4.8 12l-1.4 1.4L9 19 21 7l-1.4-1.4L9 16.2Z"
        fill="#fff"
      />
    </svg>
  );
}

function ChevronDown() {
  return (
    <svg className="stceh__selecticon" viewBox="0 0 24 24" width="24" height="24" aria-hidden="true">
      <path d="M7 10l5 5 5-5H7Z" fill="currentColor" />
    </svg>
  );
}

function Field({
  label,
  children,
  helper,
  helperUtc,
  error,
  className,
}: {
  label: ReactNode;
  children?: ReactNode;
  helper?: ReactNode;
  helperUtc?: boolean;
  error?: ReactNode;
  className?: string;
}) {
  return (
    <label className={"stceh__field" + (className ? " " + className : "")}>
      <span className="stceh__label">{label}</span>
      {children}
      {error ? (
        <span className="stceh__helper stceh__helper--error">{error}</span>
      ) : helper ? (
        <span
          className={
            "stceh__helper" + (helperUtc ? " stceh__helper--utc" : "")
          }
        >
          {helper}
        </span>
      ) : null}
    </label>
  );
}

function SelectField({
  label,
  value,
  options,
  placeholder,
  className,
  icon,
}: {
  label: ReactNode;
  value: string;
  options: Option[];
  placeholder?: ReactNode;
  className?: string;
  icon?: ReactNode;
}) {
  const selected = options.find((o) => o.value === value);
  return (
    <div className={"stceh__field" + (className ? " " + className : "")}>
      <span className="stceh__label">{label}</span>
      <div className="stceh__select" role="button" tabIndex={0}>
        <span
          className={
            "stceh__selectvalue" + (selected ? "" : " stceh__selectvalue--ph")
          }
        >
          {selected ? selected.label : placeholder}
        </span>
        {icon || <ChevronDown />}
      </div>
    </div>
  );
}

function SuccessState({ mode }: { mode: string }) {
  return (
    <div className="stceh__success">
      <div className="stceh__successbox">
        <div className="stceh__check">
          <CheckMark />
        </div>
        <p className="stceh__successmsg">
          {mode === "edit" ? COPY.editSuccessMessage : COPY.successMessage}
        </p>
        <div className="stceh__successactions">
          <button type="button" className="stceh__btn stceh__btn--light">
            {COPY.backToExplore}
          </button>
          <button type="button" className="stceh__btn stceh__btn--primary stceh__btn--minw">
            {COPY.myEvents}
          </button>
        </div>
      </div>
    </div>
  );
}

function SignInGate() {
  return (
    <div className="stceh__gate">
      <div className="stceh__gatebox">
        <p className="stceh__gatemsg">{COPY.signInMessage}</p>
        <div className="stceh__gateactions">
          <button type="button" className="stceh__btn stceh__btn--outline">
            {COPY.signIn}
          </button>
          <button type="button" className="stceh__btn stceh__btn--primary">
            {COPY.createAccount}
          </button>
        </div>
      </div>
    </div>
  );
}

function EventForm({ mode, event, communities }: { mode: string; event: EventData; communities: Community[] }) {
  const isEdit = mode === "edit";
  const isWorld = event.location === "world";
  const recLabel = RECURRENCE_OPTIONS.find((o) => o.value === event.recurrence);

  const upcoming = event.repeatEnabled
    ? [
        "Sat, Jul 18 \u{B7} 8:00 PM",
        "Sat, Jul 25 \u{B7} 8:00 PM",
        "Sat, Aug 1 \u{B7} 8:00 PM",
        "Sat, Aug 8 \u{B7} 8:00 PM",
        "Sat, Aug 15 \u{B7} 8:00 PM",
      ]
    : [];

  return (
    <div className="stceh__form">
      <div className="stceh__cols">
        <div className="stceh__leftcard">
          <div className="stceh__imagesection">
            {event.imagePreviewUrl ? (
              <div className="stceh__dropzone stceh__dropzone--filled">
                <img
                  className="stceh__preview"
                  src={event.imagePreviewUrl}
                  alt="Hangout cover preview"
                />
                <div className="stceh__previewoverlay">
                  <span className="stceh__overlaytext">{COPY.changeImage}</span>
                </div>
              </div>
            ) : (
              <div className="stceh__dropzone" role="button" tabIndex={0}>
                <div className="stceh__dzcontent">
                  <div className="stceh__icontitle">
                    <div className="stceh__cameraicon">
                      <CameraIcon />
                    </div>
                    <span className="stceh__selecttext">{COPY.selectCover}</span>
                  </div>
                  <div className="stceh__hintgroup">
                    <span className="stceh__drophint">
                      <span className="stceh__chooselink">{COPY.choosePicture}</span>{" "}
                      {COPY.dropHint}
                    </span>
                    <span className="stceh__recsize">{COPY.recommendedSize}</span>
                  </div>
                </div>
              </div>
            )}

            <button type="button" className="stceh__addvertical">
              <PlusIcon />
              <span className="stceh__addtext">
                <span className="stceh__addbold">{COPY.addVerticalCover}</span>
                <span className="stceh__addlight"> {COPY.recommendedParenthetical}</span>
              </span>
            </button>

            <div className="stceh__helperrow">
              <span className="stceh__helpericon">
                <InfoIcon size={20} />
              </span>
              <span className="stceh__helpertext">{COPY.imageHelper}</span>
            </div>
          </div>

          <div className="stceh__descfields">
            <Field label={COPY.eventName}>
              <input
                className="stceh__input"
                type="text"
                defaultValue={event.name}
                placeholder={COPY.namePlaceholder}
              />
            </Field>
            <Field label={COPY.eventDescription}>
              <textarea
                className="stceh__input stceh__textarea"
                rows={3}
                defaultValue={event.description}
                placeholder={COPY.descriptionPlaceholder}
              />
            </Field>
          </div>
        </div>

        <div className="stceh__right">
          <div className="stceh__rightfields">
            <div className="stceh__detailsblock">
              <h2 className="stceh__heading">{COPY.eventDetails}</h2>

              <div className="stceh__datetime">
                <div className="stceh__datetimerow">
                  <Field label={COPY.date} className="stceh__field--grow">
                    <div className="stceh__adornwrap">
                      <input
                        className="stceh__input"
                        type="text"
                        defaultValue={event.startDate}
                        placeholder="yyyy-mm-dd"
                      />
                      <span className="stceh__adornend">
                        <CalendarIcon />
                      </span>
                    </div>
                  </Field>
                  <Field
                    label={COPY.start}
                    className="stceh__field--grow"
                    helper={event.startTime ? "8:00 PM UTC" : undefined}
                    helperUtc
                  >
                    <div className="stceh__adornwrap">
                      <input
                        className="stceh__input"
                        type="text"
                        defaultValue={event.startTime}
                        placeholder="--:--"
                      />
                      <span className="stceh__adornend">
                        <ClockIcon />
                      </span>
                    </div>
                  </Field>
                  <SelectField
                    label={COPY.duration}
                    value={event.duration ? "dur" : ""}
                    placeholder="hh:mm"
                    className="stceh__field--grow"
                    options={
                      event.duration
                        ? [{ value: "dur", label: "10:00 PM (2 hrs)" }]
                        : []
                    }
                    icon={
                      <span className="stceh__selecticon stceh__selecticon--clock">
                        <ClockIcon />
                      </span>
                    }
                  />
                </div>

                <div className="stceh__repeatrow">
                  <span className="stceh__repeatlabel">{COPY.repeatEvent}</span>
                  <span
                    className={
                      "stceh__switch" +
                      (event.repeatEnabled ? " is-on" : "")
                    }
                    role="switch"
                    aria-checked={event.repeatEnabled}
                    aria-label={COPY.repeatEvent}
                    tabIndex={0}
                  >
                    <span className="stceh__switchtrack" />
                    <span className="stceh__switchthumb" />
                  </span>
                </div>

                {event.repeatEnabled && (
                  <div className="stceh__repeatfields">
                    <SelectField
                      label={COPY.recurrence}
                      value={event.recurrence}
                      options={RECURRENCE_OPTIONS}
                      placeholder={recLabel ? recLabel.label : ""}
                    />
                    <Field label={COPY.repeatUntil}>
                      <input
                        className="stceh__input"
                        type="text"
                        defaultValue={event.repeatEndDate}
                        placeholder="yyyy-mm-dd"
                      />
                    </Field>
                    <div className="stceh__upcoming">
                      <span className="stceh__upcominglabel">
                        {COPY.upcomingDatesLabel}
                      </span>
                      {upcoming.length > 0 ? (
                        <ul className="stceh__upcominglist">
                          {upcoming.map((d) => (
                            <li key={d} className="stceh__upcomingitem">
                              {d}
                            </li>
                          ))}
                        </ul>
                      ) : (
                        <span className="stceh__upcomingempty">
                          {COPY.upcomingDatesEmpty}
                        </span>
                      )}
                    </div>
                  </div>
                )}
              </div>
            </div>

            <div className="stceh__locationblock">
              <span className="stceh__locationlabel">{COPY.location}</span>
              <div className="stceh__locationrow">
                <SelectField
                  label={COPY.locationType}
                  value={event.location}
                  options={[
                    { value: "land", label: COPY.land },
                    { value: "world", label: COPY.world },
                  ]}
                  className="stceh__field--loc1"
                />
                {isWorld ? (
                  <SelectField
                    label={COPY.world}
                    value={event.world}
                    placeholder={COPY.worldPlaceholder}
                    className="stceh__field--loc2"
                    options={WORLD_NAMES.map((n) => ({ value: n, label: n }))}
                  />
                ) : (
                  <div className="stceh__coordsrow">
                    <Field label={COPY.latitude} className="stceh__field--grow">
                      <div className="stceh__adornwrap">
                        <span className="stceh__adornstart">X</span>
                        <input
                          className="stceh__input stceh__input--coord"
                          type="text"
                          defaultValue={event.coordX}
                        />
                      </div>
                    </Field>
                    <Field label={COPY.altitude} className="stceh__field--grow">
                      <div className="stceh__adornwrap">
                        <span className="stceh__adornstart">Y</span>
                        <input
                          className="stceh__input stceh__input--coord"
                          type="text"
                          defaultValue={event.coordY}
                        />
                      </div>
                    </Field>
                  </div>
                )}
              </div>
            </div>

            {communities.length > 0 && (
              <div className="stceh__locationblock">
                <SelectField
                  label={COPY.community}
                  value={event.communityId}
                  placeholder={COPY.communityNone}
                  options={[
                    { value: "", label: COPY.communityNone },
                    ...communities.map((c) => ({ value: c.id, label: c.name })),
                  ]}
                />
              </div>
            )}

            <div className="stceh__emailsection">
              <Field label={COPY.emailLabel}>
                <input
                  className="stceh__input"
                  type="email"
                  defaultValue={event.email}
                  placeholder={COPY.emailPlaceholder}
                />
              </Field>
            </div>
          </div>

          <div className="stceh__footer">
            <div className="stceh__reviewbar">
              <div className="stceh__reviewnotice">
                <InfoIcon size={16} />
                <span className="stceh__reviewtext">{COPY.reviewNotice}</span>
              </div>
              <button type="button" className="stceh__previewbtn">
                {COPY.preview}
                <EyeIcon />
              </button>
            </div>

            <div className="stceh__actions">
              {isEdit && (
                <button type="button" className="stceh__delete">
                  {COPY.delete}
                  <TrashIcon />
                </button>
              )}
              <button type="button" className="stceh__btn stceh__btn--cancel">
                {COPY.cancel}
              </button>
              <button type="button" className="stceh__btn stceh__btn--submit">
                {isEdit ? COPY.saveChanges : COPY.submit}
              </button>
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

type StWhatSOnCreateEditHangoutProps = {
  chrome?: boolean;
  mode?: "create" | "edit";
  state?: "form" | "signin" | "success";
};

export default function StWhatSOnCreateEditHangout({
  chrome = true,
  mode = "create",
  state = "form",
}: StWhatSOnCreateEditHangoutProps) {
  const [active] = useState<SitesNavId>("whatson");

  let body: ReactNode;
  if (state === "signin") {
    body = <SignInGate />;
  } else if (state === "success") {
    body = <SuccessState mode={mode} />;
  } else {
    const event = mode === "edit" ? EDIT_EVENT : EMPTY_EVENT;
    body = (
      <>
        <div className="stceh__headerrow">
          <button type="button" className="stceh__back" aria-label={COPY.back}>
            <BackArrowIcon />
          </button>
          <h1 className="stceh__title">
            {mode === "edit" ? COPY.editTitle : COPY.title}
          </h1>
        </div>
        <EventForm mode={mode} event={event} communities={COMMUNITIES} />
      </>
    );
  }

  return (
    <SitesChromeMaybe chrome={chrome} active={active}>
      <div className="stceh">
        <div className="stceh__bg" aria-hidden="true" />
        <div className="stceh__content">{body}</div>
      </div>
    </SitesChromeMaybe>
  );
}
