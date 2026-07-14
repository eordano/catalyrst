export const GENESIS_PLAZA_TREE = [
  {
    id: "0",
    name: "Genesis Plaza",
    expanded: true,
    children: [
      { id: "517", name: "Admin Tools", selected: true, children: [] },
      { id: "512", name: "theatre-data-source", children: [] },
      { id: "514", name: "theatre-effect-testing", children: [] },
      { id: "516", name: "Video Screen", children: [] },
      { id: "515", name: "theatre-debug-video", children: [] },
    ],
  },
];

export const GENESIS_PLAZA_INSPECTOR = {
  name: "Admin Tools",
  id: "517",
  components: [
    "core::Transform",
    "core-schema::Network-Entity",
    "core-schema::Name",
    "core-schema::Sync-Components",
    "inspector::Selection",
    "inspector::TransformConfig",
    "inspector::Config",
    "asset-packs::AdminTools",
    "asset-packs::Placeholder",
  ],
  transform: {
    position: { x: 8, y: 0, z: 17 },
    rotation: { x: 0, y: 0, z: 0 },
    scale: { x: 1, y: 1, z: 1 },
  },
};

export const GENESIS_PLAZA_CATALOG = [
  { id: "admin-toolkit", name: "Admin Toolkit", pack: "Smart Items", hue: 62, src: "assets/asset-packs/admin_tools/admin_toolkit.glb" },
  { id: "lightcube-02", name: "LightCube 02", pack: "Smart Items", hue: 62, src: "assets/asset-packs/blue_light_cube/LightCube_02/LightCube_02.glb" },
  { id: "video-player", name: "Video Player", pack: "Smart Items", hue: 325, src: "assets/asset-packs/video_screen/video_player.glb" },
  { id: "balloon001", name: "Balloon001", pack: "Models", hue: 346, src: "assets/models/Balloons/Balloon001.glb" },
  { id: "balloon002", name: "Balloon002", pack: "Models", hue: 347, src: "assets/models/Balloons/Balloon002.glb" },
  { id: "pride-balloon001", name: "Pride Balloon001", pack: "Models", hue: 177, src: "assets/models/Balloons/Pride_Balloon001.glb" },
  { id: "theatre-banners", name: "Theatre Banners", pack: "Models", hue: 201, src: "assets/models/banners/theatre-banners.glb" },
  { id: "armchairgreen", name: "ArmchairGreen", pack: "Models", hue: 298, src: "assets/models/chess/ArmchairGreen.glb" },
  { id: "chess-board", name: "Chess Board", pack: "Models", hue: 119, src: "assets/models/chess/Chess_board.glb" },
  { id: "classylamp", name: "ClassyLamp", pack: "Models", hue: 81, src: "assets/models/chess/ClassyLamp.glb" },
  { id: "table", name: "Table", pack: "Models", hue: 158, src: "assets/models/chess/Table.glb" },
  { id: "board", name: "Board", pack: "Models", hue: 190, src: "assets/models/chess/board.glb" },
  { id: "hands-01-emote", name: "Hands 01 Emote", pack: "Models", hue: 357, src: "assets/models/anims/AvatarWearablePoses/Hands_01_emote.glb" },
  { id: "simple-01-emote", name: "Simple 01 Emote", pack: "Models", hue: 31, src: "assets/models/anims/CatwalkPoses/simple_01_emote.glb" },
  { id: "marshmallow-sit-emote", name: "Marshmallow Sit Emote", pack: "Models", hue: 247, src: "assets/models/anims/Marshmallow_Sit_emote.glb" },
  { id: "birds-emote", name: "Birds Emote", pack: "Models", hue: 243, src: "assets/models/anims/Birds_emote.glb" },
  { id: "tallchair-idle-emote", name: "TallChair Idle Emote", pack: "Models", hue: 332, src: "assets/models/anims/TallChair_Idle_emote.glb" },
  { id: "wateringcan-emote", name: "WateringCan Emote", pack: "Models", hue: 150, src: "assets/models/anims/WateringCan_emote.glb" },
];

export const GENESIS_PLAZA_TITLE = "Genesis Plaza";
