CREATE TABLE [user] (
    [id] BLOB NOT NULL PRIMARY KEY,
    [username] TEXT NOT NULL UNIQUE,
    [password] TEXT,
    [reset_token] TEXT,
    [reset_token_expiry] DATETIME
);

CREATE TABLE [torrent] (
    [id] BLOB NOT NULL PRIMARY KEY,
    [v1_hash] BLOB NULL UNIQUE,
    [v2_hash] BLOB NULL UNIQUE,
    [name] TEXT NOT NULL,
    [size] INTEGER NOT NULL,
    [metainfo] BLOB NOT NULL,
    [date] DATETIME NOT NULL DEFAULT CURRENT_TIMESTAMP,
    [author] BLOB NOT NULL,
    FOREIGN KEY ([author]) REFERENCES [user]([id])
);

CREATE TABLE [file] (
    [torrent] BLOB NOT NULL,
    [path] TEXT NOT NULL,
    [size] INTEGER NOT NULL,
    PRIMARY KEY ([torrent], [path]),
    FOREIGN KEY ([torrent]) REFERENCES [torrent]([id]) ON DELETE CASCADE
);