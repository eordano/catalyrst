export type IconName =
  | "overflow"
  | "bell"
  | "backpackRotate"
  | "events"
  | "places"
  | "people"
  | "backpack"
  | "marketplace"
  | "gallery"
  | "settings"
  | "help"
  | "voice"
  | "wearables"
  | "skybox"
  | "camera"
  | "emote"
  | "friends"
  | "chat";

export type NavItem = {
  icon: IconName;
  label: string;
  shortcut?: string;
  div?: boolean;
  to?: string;
  help: string;
};

type GuideEntry = {
  icon?: IconName;
  label: string;
  shortcut?: string;
  help: string;
};

const SIDEBAR_TOP: GuideEntry[] = [
  { icon: "overflow", label: "More options", help: "Enlarge the sidebar to 150% or hide it automatically. Move to the screen edge to reveal it." },
  { label: "Profile", help: "Your name, wallet and sign-out. Opens your profile card; VIEW PROFILE opens the full passport." },
  { icon: "bell", label: "Notifications", help: "Friend requests, event reminders and rewards. The badge counts unread items." },
];

export const SIDEBAR_UPPER: NavItem[] = [
  { icon: "backpackRotate", label: "Backpack", div: true, to: "Explorer/Pages/Backpack", help: "Your avatar: equip wearables, arrange emotes and save outfits." },
  { icon: "events", label: "Events", shortcut: "X", to: "Explorer/Pages/Events", help: "Live and upcoming events; jump in with one click." },
  { icon: "places", label: "Places", shortcut: "Z", to: "Explorer/Pages/Places", help: "Browse, search and jump to places and worlds. While the minimap is hidden it also offers to show it again." },
  { icon: "people", label: "Communities", shortcut: "O", to: "Explorer/Pages/Communities", help: "Your friends, the people nearby and the communities you belong to." },
  { icon: "backpack", label: "Wearables", shortcut: "I", to: "Explorer/Pages/Backpack", help: "Opens the same Backpack page on its wearables tab; emotes are the second tab." },
  { icon: "marketplace", label: "Marketplace", to: "Explorer/Pages/Marketplace", help: "Buy wearables, emotes and names without leaving the explorer." },
  { icon: "gallery", label: "Gallery", shortcut: "K", to: "Explorer/Pages/Reel", help: "Photos you took in world, with the people and wearables in them." },
  { icon: "settings", label: "Settings", shortcut: "P", to: "Explorer/Pages/Settings", help: "Graphics, sound, controls and chat preferences." },
  { icon: "help", label: "Help & Support", div: true, to: "Explorer/Pages/Help", help: "This page: controls, shortcuts and where to get help." },
];

export const SIDEBAR_LOWER: NavItem[] = [
  { icon: "voice", label: "Voice Chat", to: "Explorer/Components/VoiceChat", help: "Nearby voice: mute or unmute and see who is talking." },
  { icon: "wearables", label: "Portable Experiences", to: "Explorer/Components/SmartWearables", help: "Smart wearables and portable experiences running right now." },
  { icon: "skybox", label: "Skybox", div: true, to: "Explorer/Components/SkyboxHUD", help: "Set the time of day." },
  { icon: "camera", label: "Camera", to: "Explorer/Pages/Camera", help: "Photo mode: fly the camera and take pictures for your reel." },
  { icon: "emote", label: "Emotes", shortcut: "B", help: "Play an emote from your wheel." },
  { icon: "friends", label: "Friends", div: true, to: "Explorer/Pages/Friends", help: "Friends, requests and blocked users. The dot means someone is online." },
  { icon: "chat", label: "Chat", shortcut: "Enter", to: "Explorer/Frames/Chat", help: "Nearby chat and direct messages. Slash commands go to the console." },
];

export function sidebarGuide(): GuideEntry[] {
  return [...SIDEBAR_TOP, ...SIDEBAR_UPPER, ...SIDEBAR_LOWER];
}
