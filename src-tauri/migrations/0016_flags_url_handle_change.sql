-- Re-point the flags_url default away from the placeholder GitHub handle
-- to the real one. Only updates rows that still hold the old placeholder
-- so we don't clobber a user-set override (set_flags_url IPC).
UPDATE meta
SET    value      = 'https://myketheguru.github.io/travis-releases/config.json',
       updated_at = CURRENT_TIMESTAMP
WHERE  key   = 'flags_url'
  AND  value = 'https://leadtoempower.github.io/travis/config.json';

UPDATE meta SET value = '16' WHERE key = 'schema_version';
