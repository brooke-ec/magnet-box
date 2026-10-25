use chrono::{DateTime, Utc};
use std::{net::IpAddr, sync::Arc};

#[derive(Clone)]
pub(crate) struct TorrentLibrary(pub(crate) Arc<scc::HashMap<[u8; 20], Torrent>>);

impl TorrentLibrary {
    pub(crate) fn new() -> Self {
        let map = Arc::new(scc::HashMap::new());
        map.insert_sync(
            [
                0x66, 0x79, 0x55, 0x0d, 0x75, 0x4d, 0x4d, 0xbc, 0x56, 0xdb, 0x6f, 0x6b, 0x34, 0x65, 0xdb, 0x29, 0xc0, 0x86, 0x99, 0x24,
            ],
            Torrent {
                created_at: Utc::now(),
                swarm: indexmap::IndexMap::new(),
                info_hash: [0; 20],
                name: "Example Torrent".to_string(),
                size: 1024 * 1024 * 1024, // 1 GiB
                leechers: 0,
                seeders: 0,
            },
        )
        .ok();

        TorrentLibrary(map)
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Torrent {
    pub(crate) created_at: chrono::DateTime<chrono::Utc>,
    pub(super) swarm: indexmap::IndexMap<[u8; 20], Peer>,
    pub(crate) info_hash: [u8; 20],
    pub(crate) leechers: usize,
    pub(crate) seeders: usize,
    pub(crate) name: String,
    pub(crate) size: u64,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct Peer {
    pub(super) last_announce: DateTime<Utc>,
    pub(super) peer_id: [u8; 20],
    pub(super) state: PeerState,
    pub(super) ip: IpAddr,
    pub(super) port: u16,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) enum PeerState {
    Leeching,
    Seeding,
}
