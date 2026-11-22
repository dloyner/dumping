-- Add migration script here
ALTER TABLE to_do_items DROP CONSTRAINT to_do_items_title_key;
ALTER TABLE to_do_items ADD CONSTRAINT to_do_items_status_chk CHECK (status IN ('PENDING', 'DONE'));
CREATE INDEX user_connections_to_do_id_idx ON user_connections (to_do_id);
