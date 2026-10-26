use chrono::{DateTime, Utc};
use rocket::tokio::try_join;
use scc::hash_map::OccupiedEntry;
use std::{collections::hash_map::RandomState, net::IpAddr, sync::Arc};
use uuid::Uuid;

type InfoHash = [u8; 20];

#[derive(Clone)]
pub(crate) struct TorrentLibrary(Arc<InnerTorrentLibrary>);

struct InnerTorrentLibrary {
    torrents: scc::HashMap<Uuid, Torrent>,
    swarms: scc::HashMap<InfoHash, Swarm>,
}

impl TorrentLibrary {
    pub(crate) async fn new() -> Self {
        let mut library = Self(Arc::new(InnerTorrentLibrary {
            torrents: scc::HashMap::new(),
            swarms: scc::HashMap::new(),
        }));

        library
            .add(Torrent {
                date: Utc::now(),
                v1_hash: [
                    0x66, 0x79, 0x55, 0x0d, 0x75, 0x4d, 0x4d, 0xbc, 0x56, 0xdb, 0x6f, 0x6b, 0x34, 0x65, 0xdb, 0x29, 0xc0, 0x86, 0x99, 0x24,
                ],
                v2_hash: [
                    0x11, 0xfc, 0x61, 0x36, 0x2a, 0xbd, 0x3a, 0x20, 0xf8, 0xf9, 0x7d, 0x1a, 0x0a, 0x9a, 0xed, 0x69, 0x47, 0x1e, 0x12, 0x99,
                ],
                leechers: 0,
                seeders: 0,
                name: "Test Torrent".to_string(),
                size: 1024,
                id: Uuid::new_v4(),
            })
            .await
            .unwrap();

        library
    }

    pub(crate) async fn add(&mut self, torrent: Torrent) -> Result<(), ()> {
        try_join!(
            async {
                self.0
                    .swarms
                    .insert_async(
                        torrent.v1_hash,
                        Swarm {
                            peers: indexmap::IndexMap::new(),
                            torrent: torrent.id,
                        },
                    )
                    .await
                    .map_err(|_| ())
            },
            async {
                self.0
                    .swarms
                    .insert_async(
                        torrent.v2_hash,
                        Swarm {
                            peers: indexmap::IndexMap::new(),
                            torrent: torrent.id,
                        },
                    )
                    .await
                    .map_err(|_| ())
            },
            async { self.0.torrents.insert_async(torrent.id, torrent.clone()).await.map_err(|_| ()) }
        )
        .map(|_| ())
    }

    #[inline]
    pub(super) async fn get_swarm(&self, info_hash: &InfoHash) -> Option<OccupiedEntry<'_, InfoHash, Swarm, RandomState>> {
        self.0.swarms.get_async(info_hash).await
    }

    #[inline]
    pub(super) async fn get_torrent(&self, id: &Uuid) -> Option<OccupiedEntry<'_, Uuid, Torrent, RandomState>> {
        self.0.torrents.get_async(id).await
    }

    #[inline]
    pub(super) async fn iter_swarms<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&mut Swarm) -> bool,
    {
        self.0.swarms.iter_mut_async(|mut e| f(&mut e)).await
    }

    #[inline]
    pub(crate) async fn iter_torrents<F>(&self, mut f: F) -> bool
    where
        F: FnMut(&mut Torrent) -> bool,
    {
        self.0.torrents.iter_mut_async(|mut e| f(&mut e)).await
    }

    #[inline]
    pub(crate) fn len(&self) -> usize {
        self.0.torrents.len()
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(crate) struct Torrent {
    pub(crate) date: chrono::DateTime<chrono::Utc>,
    pub(crate) v1_hash: InfoHash,
    pub(crate) v2_hash: InfoHash,
    pub(crate) leechers: usize,
    pub(crate) seeders: usize,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) id: Uuid,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct Swarm {
    pub(super) peers: indexmap::IndexMap<InfoHash, Peer>,
    pub(super) torrent: Uuid,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct Peer {
    pub(super) last_announce: DateTime<Utc>,
    pub(super) peer_id: InfoHash,
    pub(super) state: PeerState,
    pub(super) ip: IpAddr,
    pub(super) port: u16,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) enum PeerState {
    Leeching,
    Seeding,
}
