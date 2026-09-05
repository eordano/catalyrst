-- Full port of decentraland/quests crates/db/migrations, brought to the
-- upstream final state (all 8 migrations 20230113..20230913 folded in):
--   20230113195325_quests                          base tables
--   20230614184114_rewards                         reward hooks + items
--   20230614193400_quest-image-url                 quests.image_url
--   20230623152003_add_index_to_quest_instances    quest_instances(quest_id) index
--   20230623155919_remove_unique_from_items_..      drop UNIQUE(quest_id) on items, add index
--   20230905180813_new_creator_address_index        quests(creator_address) index
--   20230913191302_new_completed_quest_instances    completed_quest_instances table
--   20230913191439_alter_abandoned_table_name       abandoned_quests -> abandoned_quest_instances

CREATE TABLE IF NOT EXISTS quests (
  id UUID PRIMARY KEY NOT NULL,
  name TEXT NOT NULL,
  description TEXT NOT NULL,
  definition bytea NOT NULL,
  creator_address TEXT NOT NULL,
  created_at TIMESTAMP NOT NULL DEFAULT now(),
  updated_at TIMESTAMP NOT NULL DEFAULT now(),
  image_url TEXT NOT NULL DEFAULT ''
);

CREATE TABLE IF NOT EXISTS quest_instances (
  id UUID PRIMARY KEY NOT NULL,
  quest_id UUID REFERENCES quests(id),
  user_address TEXT NOT NULL,
  start_timestamp TIMESTAMP NOT NULL DEFAULT now()
);

CREATE TABLE IF NOT EXISTS events (
  id UUID PRIMARY KEY NOT NULL,
  quest_instance_id UUID REFERENCES quest_instances(id),
  user_address TEXT NOT NULL,
  event bytea NOT NULL,
  timestamp TIMESTAMP NOT NULL DEFAULT now()
);

-- 20230913191439: the table is named abandoned_quest_instances in the final state.
CREATE TABLE IF NOT EXISTS abandoned_quest_instances (
  id UUID PRIMARY KEY NOT NULL,
  quest_instance_id UUID REFERENCES quest_instances(id),
  created_at TIMESTAMP NOT NULL DEFAULT now(),
  UNIQUE (quest_instance_id)
);
-- If a pre-rename deployment created abandoned_quests, fold it into the new name.
DO $$
BEGIN
  IF EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'abandoned_quests')
     AND NOT EXISTS (SELECT 1 FROM information_schema.tables WHERE table_name = 'abandoned_quest_instances') THEN
    ALTER TABLE abandoned_quests RENAME TO abandoned_quest_instances;
  END IF;
END $$;

-- 20230913191302: completed instances tracking (used by complete_quest_instance / is_completed).
CREATE TABLE IF NOT EXISTS completed_quest_instances (
  id UUID PRIMARY KEY NOT NULL,
  quest_instance_id UUID REFERENCES quest_instances(id),
  created_at TIMESTAMP NOT NULL DEFAULT now(),
  UNIQUE (quest_instance_id)
);

CREATE TABLE IF NOT EXISTS deactivated_quests (
  id UUID PRIMARY KEY NOT NULL,
  quest_id UUID REFERENCES quests(id),
  created_at TIMESTAMP NOT NULL DEFAULT now(),
  UNIQUE (quest_id)
);

CREATE TABLE IF NOT EXISTS quest_updates (
  id UUID PRIMARY KEY NOT NULL,
  quest_id UUID REFERENCES quests(id),
  previous_quest_id UUID REFERENCES quests(id),
  created_at TIMESTAMP NOT NULL DEFAULT now(),
  UNIQUE (quest_id)
);

CREATE TABLE IF NOT EXISTS quest_reward_hooks (
  quest_id UUID REFERENCES quests(id),
  webhook_url TEXT NOT NULL,
  request_body JSON,
  UNIQUE (quest_id)
);

-- 20230623155919: reward items lost UNIQUE(quest_id) so a quest may have many
-- items; a non-unique index replaces the lookup path.
CREATE TABLE IF NOT EXISTS quest_reward_items (
  quest_id UUID REFERENCES quests(id),
  reward_name TEXT NOT NULL,
  reward_image TEXT NOT NULL
);
-- Drop a stale UNIQUE(quest_id) if a prior schema created it under either the
-- sqlx-generated or an ad-hoc constraint name.
DO $$
DECLARE
  con TEXT;
BEGIN
  SELECT conname INTO con
  FROM pg_constraint
  WHERE conrelid = 'quest_reward_items'::regclass AND contype = 'u';
  IF con IS NOT NULL THEN
    EXECUTE format('ALTER TABLE quest_reward_items DROP CONSTRAINT %I', con);
  END IF;
END $$;

-- Upstream indexes (20230623152003, 20230623155919, 20230905180813).
CREATE INDEX IF NOT EXISTS quest_instances_quest_id_idx ON quest_instances(quest_id);
CREATE INDEX IF NOT EXISTS quest_reward_items_quest_id_idx ON quest_reward_items(quest_id);
CREATE INDEX IF NOT EXISTS quest_creator_address_idx ON quests(creator_address);
