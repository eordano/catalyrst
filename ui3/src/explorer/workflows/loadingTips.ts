import cameraArt from "./loading-tips/Camera.webp";
import marketplaceCreditsArt from "./loading-tips/MarketplaceCredits.webp";
import creatorHubArt from "./loading-tips/CreatorHub.webp";
import badgesArt from "./loading-tips/Badges.webp";
import emotesArt from "./loading-tips/Emotes.webp";
import wearablesArt from "./loading-tips/Wearables.webp";
import communitiesArt from "./loading-tips/Communities.webp";
import eventsArt from "./loading-tips/Events.webp";
import worldsArt from "./loading-tips/Worlds.webp";
import hangOutArt from "./loading-tips/HangOut.webp";

export type LoadingTip = { title: string; body: string; art: string };

export const LOADING_TIPS: LoadingTip[] = [
  {
    title: "Take a Shot",
    body:
      "See something worth remembering? Press ‘C’ to open the Camera and " +
      "‘Space bar’ to snap a photo.",
    art: cameraArt,
  },
  {
    title: "Show Up",
    body:
      "Spend time in Decentraland and earn Credits. Use them in the " +
      "Marketplace to pick up Wearables and Emotes, no purchase needed. " +
      "Shape your look as you go.",
    art: marketplaceCreditsArt,
  },
  {
    title: "Build Something",
    body:
      "The Creator Hub gives you tools to build your own spaces, from simple " +
      "hangouts to bigger experiences. What you build can become someone’s " +
      "regular spot.",
    art: creatorHubArt,
  },
  {
    title: "Your Presence",
    body:
      "Badges reflect how you've spent time here: socializing, creating, or " +
      "just being around. They show up on your profile so others get a sense " +
      "of who they're meeting.",
    art: badgesArt,
  },
  {
    title: "Say Hi!",
    body:
      "Emotes let you wave, react, or show off your moves without saying a " +
      "word. Press ‘B’ to open the Emote Wheel and join the moment.",
    art: emotesArt,
  },
  {
    title: "Your Look",
    body:
      "Wearables shape how you appear over time. Made by the community, they " +
      "become part of how people recognize you—and how you show off your " +
      "style.",
    art: wearablesArt,
  },
  {
    title: "Your People",
    body:
      "Communities are how you find your people — from dance parties and " +
      "chess matches to language practice, late-night talks, and art tours. " +
      "Show up a few times and you start recognizing who’s there.",
    art: communitiesArt,
  },
  {
    title: "What's On",
    body:
      "Movie nights, trivia, dance parties, there's usually something " +
      "happening. Drop in enough times and you'll start to recognize the " +
      "regulars.",
    art: eventsArt,
  },
  {
    title: "Your Space",
    body:
      "Your World is yours to do what you want with: build, experiment, hang " +
      "out, host. You can also wander into other people's Worlds and see " +
      "what they've put together.",
    art: worldsArt,
  },
  {
    title: "Hang Out",
    body:
      "Genesis Plaza is the place people tend to hang—around the fire pit, " +
      "in conversation, crossing paths, feeding pigeons. Come by and see " +
      "who’s around!",
    art: hangOutArt,
  },
];

export const TIP_ROTATION_MS = 10000;
