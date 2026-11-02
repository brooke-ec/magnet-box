CREATE TABLE [torrent] (
    [info_hash] BLOB NOT NULL PRIMARY KEY,
    [name] TEXT NOT NULL,
    [size] INTEGER NOT NULL,
    [metainfo] BLOB NOT NULL,
    [created_at] DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP
);

CREATE TABLE [user] (
    [id] BLOB NOT NULL PRIMARY KEY,
    [username] TEXT NOT NULL UNIQUE,
    [password] TEXT,
    [reset_token] TEXT,
    [reset_token_expiry] DATETIME
);