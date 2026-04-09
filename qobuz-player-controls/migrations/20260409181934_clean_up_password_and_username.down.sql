ALTER TABLE credentials ADD COLUMN password TEXT;
ALTER TABLE credentials ADD COLUMN username TEXT UNIQUE;
