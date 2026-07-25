-- Stage-1 static fixture so the Unity passport renders against a fresh DB.
-- These mirror well-known decentraland badges; replace with the authoritative
-- definitions once the private decentraland/badges repo is available.
-- Idempotent: ON CONFLICT DO NOTHING so re-runs / Stage-2 seeds don't clobber.

INSERT INTO badge_definitions (id, name, description, category, is_tier, assets) VALUES
  ('open_for_business',
   'Open for Business',
   'Set up a store and publish a collection.',
   'Builder',
   false,
   '{"2d":{"normal":"https://badges.decentraland.org/assets/open_for_business/2d/normal.png","hrm":"https://badges.decentraland.org/assets/open_for_business/2d/hrm.png","baseColor":"https://badges.decentraland.org/assets/open_for_business/2d/baseColor.png"},"3d":{"normal":"https://badges.decentraland.org/assets/open_for_business/3d/normal.png","hrm":"https://badges.decentraland.org/assets/open_for_business/3d/hrm.png","baseColor":"https://badges.decentraland.org/assets/open_for_business/3d/baseColor.png"}}'::jsonb),
  ('decentraland_citizen',
   'Decentraland Citizen',
   'Log in to Decentraland for the first time.',
   'Explorer',
   false,
   '{"2d":{"normal":"https://badges.decentraland.org/assets/decentraland_citizen/2d/normal.png","hrm":"","baseColor":""},"3d":{"normal":"","hrm":"","baseColor":""}}'::jsonb),
  ('walkabout',
   'Walkabout',
   'Walk a cumulative distance across Decentraland.',
   'Explorer',
   true,
   '{"2d":{"normal":"https://badges.decentraland.org/assets/walkabout/2d/normal.png","hrm":"","baseColor":""},"3d":{"normal":"","hrm":"","baseColor":""}}'::jsonb),
  ('emotionista',
   'Emotionista',
   'Play emotes in-world.',
   'Socializer',
   true,
   '{"2d":{"normal":"https://badges.decentraland.org/assets/emotionista/2d/normal.png","hrm":"","baseColor":""},"3d":{"normal":"","hrm":"","baseColor":""}}'::jsonb)
ON CONFLICT (id) DO NOTHING;

INSERT INTO badge_tiers (badge_id, tier_id, tier_name, description, assets, criteria_steps, ordinal) VALUES
  ('walkabout', 'walkabout-starter',  'Starter',  'Walk 1,000 meters.',     '{"2d":{"normal":"https://badges.decentraland.org/assets/walkabout/starter/2d/normal.png"}}'::jsonb,   1000, 0),
  ('walkabout', 'walkabout-bronze',   'Bronze',   'Walk 10,000 meters.',    '{"2d":{"normal":"https://badges.decentraland.org/assets/walkabout/bronze/2d/normal.png"}}'::jsonb,   10000, 1),
  ('walkabout', 'walkabout-silver',   'Silver',   'Walk 100,000 meters.',   '{"2d":{"normal":"https://badges.decentraland.org/assets/walkabout/silver/2d/normal.png"}}'::jsonb,  100000, 2),
  ('walkabout', 'walkabout-gold',     'Gold',     'Walk 1,000,000 meters.', '{"2d":{"normal":"https://badges.decentraland.org/assets/walkabout/gold/2d/normal.png"}}'::jsonb, 1000000, 3),
  ('emotionista', 'emotionista-bronze', 'Bronze', 'Play 10 emotes.',  '{"2d":{"normal":"https://badges.decentraland.org/assets/emotionista/bronze/2d/normal.png"}}'::jsonb,  10, 0),
  ('emotionista', 'emotionista-silver', 'Silver', 'Play 100 emotes.', '{"2d":{"normal":"https://badges.decentraland.org/assets/emotionista/silver/2d/normal.png"}}'::jsonb, 100, 1),
  ('emotionista', 'emotionista-gold',   'Gold',   'Play 1000 emotes.','{"2d":{"normal":"https://badges.decentraland.org/assets/emotionista/gold/2d/normal.png"}}'::jsonb, 1000, 2)
ON CONFLICT (badge_id, tier_id) DO NOTHING;
