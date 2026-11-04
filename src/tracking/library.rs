use chrono::{DateTime, Utc};
use rocket::tokio::try_join;
use scc::hash_map::OccupiedEntry;
use serde::Serialize;
use std::{collections::hash_map::RandomState, net::IpAddr, sync::Arc};
use uuid::Uuid;

pub(crate) type InfoHash = [u8; 20];

#[derive(Clone)]
pub(crate) struct TorrentLibrary(Arc<InnerTorrentLibrary>);

struct InnerTorrentLibrary {
    torrents: scc::HashMap<Uuid, Torrent>,
    swarms: scc::HashMap<InfoHash, Swarm>,
}

impl TorrentLibrary {
    pub(crate) async fn new() -> Self {
        Self(Arc::new(InnerTorrentLibrary {
            torrents: scc::HashMap::new(),
            swarms: scc::HashMap::new(),
        }))
    }

    pub(crate) async fn add(
        &self,
        id: Uuid,
        v1_hash: Option<InfoHash>,
        v2_hash: Option<InfoHash>,
        name: String,
        author: String,
        size: u64,
        date: DateTime<Utc>,
        magnet: String,
    ) -> Result<(), ()> {
        try_join!(
            async {
                if let Some(v1_hash) = v1_hash {
                    self.0
                        .swarms
                        .insert_async(
                            v1_hash,
                            Swarm {
                                peers: indexmap::IndexMap::new(),
                                hash: v1_hash,
                                torrent: id,
                            },
                        )
                        .await
                        .map_err(|_| ())
                } else {
                    Ok(())
                }
            },
            async {
                if let Some(v2_hash) = v2_hash {
                    self.0
                        .swarms
                        .insert_async(
                            v2_hash,
                            Swarm {
                                peers: indexmap::IndexMap::new(),
                                hash: v2_hash,
                                torrent: id,
                            },
                        )
                        .await
                        .map_err(|_| ())
                } else {
                    Ok(())
                }
            },
            async {
                self.0
                    .torrents
                    .insert_async(
                        id,
                        Torrent {
                            leechers: 0,
                            seeders: 0,
                            v1_hash,
                            v2_hash,
                            magnet,
                            date,
                            author,
                            name,
                            size,
                            id,
                        },
                    )
                    .await
                    .map_err(|_| ())
            }
        )
        .map(|_| ())
    }

    pub(crate) async fn remove(&self, id: &Uuid) -> Result<(), ()> {
        let torrent = self.0.torrents.remove_async(id).await.ok_or(())?.1;

        try_join!(
            async {
                if let Some(v1_hash) = torrent.v1_hash {
                    self.0.swarms.remove_async(&v1_hash).await.ok_or(())
                } else {
                    Err(())
                }
            },
            async {
                if let Some(v2_hash) = torrent.v2_hash {
                    self.0.swarms.remove_async(&v2_hash).await.ok_or(())
                } else {
                    Err(())
                }
            }
        )
        .map(|_| ())
    }

    #[inline]
    pub(super) async fn get_swarm(&self, info_hash: &InfoHash) -> Option<OccupiedEntry<'_, InfoHash, Swarm, RandomState>> {
        self.0.swarms.get_async(info_hash).await
    }

    #[inline]
    pub(crate) async fn get_torrent(&self, id: &Uuid) -> Option<OccupiedEntry<'_, Uuid, Torrent, RandomState>> {
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

#[derive(Debug, PartialEq, Eq, Clone, Serialize)]
pub(crate) struct Torrent {
    pub(crate) date: chrono::DateTime<chrono::Utc>,
    pub(crate) v1_hash: Option<InfoHash>,
    pub(crate) v2_hash: Option<InfoHash>,
    pub(crate) leechers: usize,
    pub(crate) magnet: String,
    pub(crate) seeders: usize,
    pub(crate) author: String,
    pub(crate) name: String,
    pub(crate) size: u64,
    pub(crate) id: Uuid,
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub(super) struct Swarm {
    pub(super) peers: indexmap::IndexMap<InfoHash, Peer>,
    pub(super) hash: InfoHash,
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
