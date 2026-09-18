import type { SidebarDesignIconName } from "./SidebarDesignIcon";

export const SIDEBAR_GUIDE: { label: string; icon: SidebarDesignIconName; help: string; shortcut?: string }[] = [
  { label: "Me", icon: "home", help: "Home, Backpack, Profile, Badges, Gallery and Notifications." },
  { label: "Location", icon: "pin", help: "Pin the minimap, copy coordinates or a jump link, contact scene creators and change the time of day." },
  { label: "Discover", icon: "compass", help: "Find Events, Places, Communities and Marketplace. Open the Map from Location." },
  { label: "System", icon: "settings", help: "Settings, feature flags, controls, sidebar guide and support." },
  { label: "Camera", icon: "camera", help: "Take a photo or open your camera reel." },
  { label: "Friends", icon: "friends", help: "View friends and requests when signed in." },
  { label: "Emotes", icon: "emote", shortcut: "B", help: "Open the emote wheel." },
  { label: "Voice chat", icon: "voice", help: "Turn your microphone on or off." },
  { label: "Audio", icon: "audio", help: "Adjust volume and advanced audio settings." },
  { label: "Chat", icon: "chat", shortcut: "Enter", help: "Nearby chat, private conversations and communities." },
  { label: "Portables", icon: "bag", help: "Manage active portable experiences when present." },
];
