-- Add migration script here
ALTER TABLE to_do_items ADD COLUMN user_id TEXT;
ALTER TABLE to_do_items ADD COLUMN created_at TIMESTAMPTZ NOT NULL DEFAULT now();
UPDATE to_do_items t SET user_id = uc.user_id
FROM user_connections uc WHERE uc.to_do_id = t.id;
DELETE FROM to_do_items WHERE user_id IS NULL;
ALTER TABLE to_do_items ALTER column user_id SET NOT NULL;
CREATE INDEX to_do_items_user_id_idx ON to_do_items (user_id, id);
DROP TABLE user_connections;
