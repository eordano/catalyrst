import badgeGlyph from "./sidebar-assets/badge-glyph.svg?url";
import bag from "./sidebar-assets/bag.svg?url";
import bell from "./sidebar-assets/bell.svg?url";
import bolt from "./sidebar-assets/bolt.svg?url";
import bugGlyph from "./sidebar-assets/bug-glyph.svg?url";
import calendarGlyph from "./sidebar-assets/calendar-glyph.svg?url";
import camera from "./sidebar-assets/camera.svg?url";
import chatGlyph from "./sidebar-assets/chat-glyph.svg?url";
import collapseGlyph from "./sidebar-assets/collapse-glyph.svg?url";
import compass from "./sidebar-assets/compass.svg?url";
import discordGlyph from "./sidebar-assets/discord-glyph.svg?url";
import emote from "./sidebar-assets/emote.svg?url";
import exitGlyph from "./sidebar-assets/exit-glyph.svg?url";
import friends from "./sidebar-assets/friends.svg?url";
import heartFilled from "./sidebar-assets/heart-filled-glyph.svg?url";
import heartGlyph from "./sidebar-assets/heart-glyph.svg?url";
import heart from "./sidebar-assets/heart.svg?url";
import helpGlyph from "./sidebar-assets/help-glyph.svg?url";
import keyboardGlyph from "./sidebar-assets/keyboard-glyph.svg?url";
import locationGlyph from "./sidebar-assets/location-glyph.svg?url";
import more from "./sidebar-assets/more.svg?url";
import packGlyph from "./sidebar-assets/pack-glyph.svg?url";
import peopleGlyph from "./sidebar-assets/people-glyph.svg?url";
import pinGlyph from "./sidebar-assets/pin-glyph.svg?url";
import powerGlyph from "./sidebar-assets/power-glyph.svg?url";
import sceneOptionsGlyph from "./sidebar-assets/sceneOptions-glyph.svg?url";
import settings from "./sidebar-assets/settings.svg?url";
import slidersGlyph from "./sidebar-assets/sliders-glyph.svg?url";
import sunGlyph from "./sidebar-assets/sun-glyph.svg?url";
import supportGlyph from "./sidebar-assets/support-glyph.svg?url";
import userGlyph from "./sidebar-assets/user-glyph.svg?url";
import voice from "./sidebar-assets/voice.svg?url";

const buttonIcons = { bag, bell, bolt, camera, compass, emote, friends, heart, more, settings, voice };
const glyphIcons = { heartFilled, badge: badgeGlyph, bug: bugGlyph, calendar: calendarGlyph, chat: chatGlyph, collapse: collapseGlyph, discord: discordGlyph, exit: exitGlyph, heart: heartGlyph, help: helpGlyph, keyboard: keyboardGlyph, location: locationGlyph, pack: packGlyph, people: peopleGlyph, pin: pinGlyph, power: powerGlyph, sceneOptions: sceneOptionsGlyph, sliders: slidersGlyph, sun: sunGlyph, support: supportGlyph, user: userGlyph };

export type SidebarDesignIconName = keyof typeof buttonIcons | keyof typeof glyphIcons | "home" | "copy" | "close" | "flag" | "audio" | "gallery";

export default function SidebarDesignIcon({ name, button = false }: { name: SidebarDesignIconName; button?: boolean }) {
  if (name === "gallery") return <svg className="sd__icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true"><rect x="3" y="3" width="18" height="18" rx="3" /><circle cx="8" cy="8" r="1.5" /><path d="m3 17 5-5 4 4 4-6 5 7" /></svg>;
  if (name === "home" || name === "bell") return <svg className="sd__icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{name === "home" ? <><path d="m3 10 9-7 9 7v11h-6v-7H9v7H3Z" /></> : <><path d="M18 8a6 6 0 0 0-12 0c0 7-3 7-3 9h18c0-2-3-2-3-9Z" /><path d="M10 21h4" /></>}</svg>;
  if (name === "pin" || name === "chat" || name === "bag") return <svg className="sd__icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">
    {name === "pin" ? <><path d="M19 10c0 5-7 11-7 11S5 15 5 10a7 7 0 1 1 14 0Z" /><circle cx="12" cy="10" r="2" /></> : name === "chat" ? <path d="M4 4h16v12H9l-5 4V4Z" /> : <><path d="M5 7h14v14H5Z" /><path d="M8 8V6a4 4 0 0 1 8 0v2" /></>}
  </svg>;
  const icons: Partial<Record<SidebarDesignIconName, string>> = button ? buttonIcons : glyphIcons;
  const src = icons[name] ?? (glyphIcons as Partial<Record<SidebarDesignIconName, string>>)[name] ?? (buttonIcons as Partial<Record<SidebarDesignIconName, string>>)[name];
  if (src) return <img className={button && name in buttonIcons ? "sd__button-art" : "sd__icon"} src={src} alt="" aria-hidden="true" />;
  return <svg className="sd__icon" viewBox="0 0 24 24" fill="none" stroke="currentColor" strokeWidth="1.7" strokeLinecap="round" strokeLinejoin="round" aria-hidden="true">{name === "copy" ? <><rect x="8" y="8" width="13" height="13" rx="2" /><path d="M16 8V3H3v13h5" /></> : name === "audio" ? <><path d="M4 9v6h4l5 4V5L8 9H4Z" /><path d="M17 8a6 6 0 0 1 0 8m3-11a10 10 0 0 1 0 14" /></> : name === "flag" ? <path d="M5 21V3c5-4 9 4 14 0v10c-5 4-9-4-14 0" /> : <path d="m7 7 10 10M7 17 17 7" />}</svg>;
}
