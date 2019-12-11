-- This file should undo anything in `up.sql`

DROP EXTENSION IF EXISTS "uuid-ossp" CASCADE;
DROP EXTENSION IF EXISTS "pgcrypto" CASCADE;
DROP EXTENSION IF EXISTS "hstore" CASCADE;

DROP TABLE IF EXISTS "media" CASCADE;
DROP TABLE IF EXISTS "space_members" CASCADE;
DROP TABLE IF EXISTS "channel_members" CASCADE;
DROP TABLE IF EXISTS "spaces" CASCADE;
DROP TABLE IF EXISTS "users" CASCADE;
DROP TABLE IF EXISTS "channels" CASCADE;
DROP TABLE IF EXISTS "messages" CASCADE;
DROP TABLE IF EXISTS "restrained_members" CASCADE;