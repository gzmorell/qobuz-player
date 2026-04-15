
ALTER TABLE configuration RENAME TO configuration_old;

CREATE TABLE configuration (
    max_audio_quality INTEGER NOT NULL DEFAULT 27
);

INSERT INTO configuration (max_audio_quality)
SELECT COALESCE(max_audio_quality, 27)
FROM configuration_old;

DROP TABLE configuration_old;
