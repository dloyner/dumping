-- Add migration script here
UPDATE users SET email = lower(btrim(email));
CREATE UNIQUE INDEX users_email_lower_uq ON users(lower(email));
