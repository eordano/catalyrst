export const TRIGGER_CHIP: Record<string, string> = {
  on_click: "when clicked",
  on_input_action: "when E is pressed",
  on_player_enters_area: "when a player enters",
  on_player_leaves_area: "when a player leaves",
  on_spawn: "on spawn",
};

export const ACTION_CHIP: Record<string, string> = {
  start_tween: "move it",
  set_visibility: "show / hide",
  play_sound: "play a sound",
  play_animation: "play an animation",
  teleport_player: "teleport the player",
  open_link: "open a link",
};

export const TRIGGER_ID = {
  click: "on_click",
  input: "on_input_action",
} as const;

export const ACTION_ID = {
  tween: "start_tween",
  visibility: "set_visibility",
  sound: "play_sound",
  animate: "play_animation",
} as const;
