-- deployment/init-databases.sql
-- Executed by run_deployment.sh with psql, connected to the `to_do` database.
-- RDS creates only `to_do`; the application also needs a separate `auth` database
-- (each database keeps its own sqlx migration history). Safe to run repeatedly.
SELECT 'CREATE DATABASE auth'
WHERE NOT EXISTS (SELECT FROM pg_database WHERE datname = 'auth')\gexec
