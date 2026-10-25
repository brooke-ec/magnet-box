CREATE TABLE [torrents] (
    [info_hash] BLOB NOT NULL PRIMARY KEY,
    [name] TEXT NOT NULL,
    [size] INTEGER NOT NULL,
    [metainfo] BLOB NOT NULL,
    [created_at] DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
)