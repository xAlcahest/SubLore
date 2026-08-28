-- Rows a version 1 database holds before it is migrated forward. Loaded by
-- crates/sublore-project/tests/migrations.rs. Real-shaped on purpose: quoted and non-Latin titles,
-- a path with spaces, a non-ASCII file name, a Windows path, and the NULLs the schema allows.
-- See BACKLOG.md M4.1.

INSERT INTO series (id, title, created_at) VALUES
    (1, 'Kaguya''s Notes', 1756300000);

INSERT INTO episodes (id, series_id, ordinal, title, created_at) VALUES
    (1, 1, 1, 'Episode 1: "The Arrival"', 1756300001),
    (2, 1, 2, '第二話 かぐや姫', 1756300002),
    (3, 1, 3, 'Épisode 3: l''arrivée', 1756300003);

INSERT INTO episode_files (id, episode_id, role, path, byte_length, modified_at, added_at) VALUES
    (1, 1, 'media', '/home/user/Series One/ep01.mkv', 734003200, 1756200000, 1756300010),
    (2, 1, 'source', '/home/user/Series One/ep01.en.srt', 18342, 1756200001, 1756300011),
    (3, 1, 'target', '/home/user/Series One/ep01.it.srt', NULL, NULL, 1756300012),
    (4, 2, 'media', '/home/user/Series One/第2話.mkv', 812345678, 1756200002, 1756300013),
    (5, 2, 'source', 'C:\Users\user\Series One\ep02 - source.vtt', 20481, 1756200003, 1756300014),
    (6, 3, 'target', '/home/user/Séries One/ép03.ass', 33107, NULL, 1756300015);
