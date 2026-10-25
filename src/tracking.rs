//! This module contains all endpoints for the BitTorrent tracker.

use chrono::{DateTime, Utc};
use rand::seq::index::sample;
use rocket::{
    Request, State,
    fairing::AdHoc,
    futures::future::BoxFuture,
    http::{ContentType, Status, uri::Origin},
    response::Responder,
    tokio::time,
};
use serde::{Deserialize, Serialize};
use std::{net::IpAddr, sync::Arc, time::Duration};

use crate::configuration::AppConfiguration;

macro_rules! err {
    ($expr:expr) => {
        BencodeError { reason: $expr.to_string() }
    };
}

/// Returns a fairing that mounts the tracker endpoints.
pub(crate) fn faring() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("tracker", |rocket| async {
        rocket
            .attach(AdHoc::on_liftoff("tracker:bookkeeping", bookkeeping))
            .mount("/", routes![announce])
    })
}

/// Starts a background task that performs bookkeeping for the tracker.
fn bookkeeping<'a>(rocket: &'a rocket::Rocket<rocket::Orbit>) -> BoxFuture<'a, ()> {
    let interval = rocket.state::<AppConfiguration>().unwrap().interval;
    let library = rocket.state::<TorrentLibrary>().unwrap().clone();
    let mut shutdown = rocket.shutdown();

    Box::pin(async move {
        rocket::tokio::spawn(async move {
            let mut wait = time::interval(Duration::from_secs(interval));
            loop {
                rocket::tokio::select! {
                    _ = &mut shutdown => {
                        rocket::info!("Stopping tracker bookkeeping...");
                        break;
                    }
                    _ = wait.tick() => {
                        rocket::debug!("Cleaning up stale peers...");
                        let now = Utc::now();
                        library
                            .0
                            .iter_mut_async(|mut torrent| {
                                let mut removed_leechers = 0;
                                let mut removed_seeders = 0;

                                torrent.swarm.retain(|_, peer| {
                                    let elapsed = now.signed_duration_since(peer.last_announce);
                                    let retain = elapsed.num_seconds() < (interval as i64) * 2;

                                    if !retain {
                                        match peer.state {
                                            PeerState::Leeching => removed_leechers += 1,
                                            PeerState::Seeding => removed_seeders += 1,
                                        }
                                    }

                                    retain
                                });

                                torrent.leechers -= removed_leechers;
                                torrent.seeders -= removed_seeders;

                                true
                            })
                            .await;
                    }
                }
            }
        });
    })
}

/// The announce endpoint for the BitTorrent tracker.
#[get("/announce")]
async fn announce<'r>(
    uri: &'r Origin<'r>,
    ip: IpAddr,
    library: &State<TorrentLibrary>,
    config: &State<AppConfiguration>,
) -> BencodeResult<AnnounceResponse> {
    let query: AnnounceQuery = serde_querystring::from_bytes(
        uri.query().ok_or(err!("Invalid parameters"))?.as_bytes(),
        serde_querystring::ParseMode::UrlEncoded,
    )?;

    let info_hash: [u8; 20] = query.info_hash.as_ref().try_into().map_err(|_| err!("Invalid info_hash length"))?;
    let peer_id: [u8; 20] = query.peer_id.as_ref().try_into().map_err(|_| err!("Invalid peer_id length"))?;
    let mut torrent = library.0.get_async(&info_hash).await.ok_or(err!("Torrent not allowed"))?;

    if query.event == Some(AnnounceEvent::Stopped) {
        if let Some(peer) = torrent.swarm.swap_remove(&peer_id) {
            match peer.state {
                PeerState::Leeching => torrent.leechers -= 1,
                PeerState::Seeding => torrent.seeders -= 1,
            }
        }

        Err(err!("Peer removed from swarm"))
    } else {
        let new_state = match query.left {
            0 => PeerState::Seeding,
            _ => PeerState::Leeching,
        };

        let old_state = torrent
            .swarm
            .insert(
                peer_id,
                Peer {
                    ip: query.ip.unwrap_or(ip),
                    last_announce: Utc::now(),
                    state: new_state.clone(),
                    port: query.port,
                    peer_id,
                }, // Insert the peer into the swarm, replacing any existing entry
            )
            .map(|peer| peer.state);

        match (old_state, new_state) {
            (Some(PeerState::Leeching), PeerState::Seeding) => {
                torrent.leechers -= 1;
                torrent.seeders += 1;
            }
            (Some(PeerState::Seeding), PeerState::Leeching) => {
                torrent.leechers += 1;
                torrent.seeders -= 1;
            }
            (None, PeerState::Leeching) => {
                torrent.leechers += 1;
            }
            (None, PeerState::Seeding) => {
                torrent.seeders += 1;
            }
            _ => {}
        }

        let peer_index = torrent.swarm.get_index_of(&peer_id).expect("inserted peer must exist");
        let candidate_len = torrent.swarm.len() - 1;
        let numwant = query.numwant.unwrap_or(config.numwant).min(candidate_len);
        let peers = sample(&mut rand::rng(), candidate_len, numwant)
            .into_iter()
            .filter_map(|i| {
                let swarm_index = if i >= peer_index { i + 1 } else { i };
                torrent.swarm.get_index(swarm_index).map(|(_, peer)| peer)
            })
            .collect::<Vec<&Peer>>();

        rocket::debug!(
            "Announce from {}:{}, returned {} peers",
            query.ip.unwrap_or(ip),
            query.port,
            peers.len()
        );

        let peers = match query.compact {
            Some(0) => AnnounceResponsePeerList::wide(peers),
            _ => AnnounceResponsePeerList::compact(peers),
        };

        BencodeResult::Ok(Bencode(AnnounceResponse {
            interval: config.interval,
            peers,
        }))
    }
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
struct AnnounceQuery {
    info_hash: serde_bytes::ByteBuf,
    peer_id: serde_bytes::ByteBuf,
    ip: Option<IpAddr>,
    port: u16,
    uploaded: u64,
    downloaded: u64,
    left: u64,
    numwant: Option<usize>,
    event: Option<AnnounceEvent>,
    compact: Option<u8>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
enum AnnounceEvent {
    #[serde(rename = "started")]
    Started,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "stopped")]
    Stopped,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct AnnounceResponse {
    #[serde(flatten)]
    peers: AnnounceResponsePeerList,
    interval: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
enum AnnounceResponsePeerList {
    Compact {
        #[serde(with = "serde_bytes", rename = "peers")]
        peers4: Vec<u8>,
        #[serde(with = "serde_bytes")]
        peers6: Vec<u8>,
    },
    Wide {
        peers: Vec<AnnounceResponsePeer>,
    },
}

impl AnnounceResponsePeerList {
    fn compact(peers: Vec<&Peer>) -> Self {
        let mut peers4 = Vec::new();
        let mut peers6 = Vec::new();

        for peer in peers {
            match peer.ip {
                IpAddr::V4(ip) => {
                    peers4.extend_from_slice(&ip.octets());
                    peers4.extend_from_slice(&peer.port.to_be_bytes());
                }
                IpAddr::V6(ip) => {
                    peers6.extend_from_slice(&ip.octets());
                    peers6.extend_from_slice(&peer.port.to_be_bytes());
                }
            }
        }

        AnnounceResponsePeerList::Compact { peers4, peers6 }
    }

    fn wide(peers: Vec<&Peer>) -> Self {
        let peers = peers
            .into_iter()
            .map(|peer| AnnounceResponsePeer {
                peer_id: peer.peer_id.to_vec(),
                ip: peer.ip.to_string(),
                port: peer.port,
            })
            .collect();

        AnnounceResponsePeerList::Wide { peers }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct AnnounceResponsePeer {
    #[serde(with = "serde_bytes")]
    peer_id: Vec<u8>,
    ip: String,
    port: u16,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct BencodeError {
    #[serde(rename = "failure reason")]
    reason: String,
}

impl<'r> Responder<'r, 'static> for BencodeError {
    fn respond_to(self, request: &'r Request<'_>) -> rocket::response::Result<'static> {
        Bencode(self).respond_to(request)
    }
}

impl<T: std::error::Error> From<T> for BencodeError {
    fn from(error: T) -> Self {
        BencodeError { reason: error.to_string() }
    }
}

type BencodeResult<T> = Result<Bencode<T>, BencodeError>;

#[derive(Debug, PartialEq, Eq)]
struct Bencode<T>(T);

impl<'r, T: Serialize> Responder<'r, 'static> for Bencode<T> {
    fn respond_to(self, req: &'r Request<'_>) -> rocket::response::Result<'static> {
        let bytes = serde_bencoded::to_vec(&self.0).map_err(|e| {
            eprintln!("Failed to serialize to bencode: {}", e);
            Status::InternalServerError
        })?;

        let mut response = bytes.respond_to(req)?;
        response.set_header(ContentType::Plain);
        Ok(response)
    }
}

#[derive(Clone)]
pub(crate) struct TorrentLibrary(Arc<scc::HashMap<[u8; 20], Torrent>>);

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
struct Torrent {
    created_at: chrono::DateTime<chrono::Utc>,
    swarm: indexmap::IndexMap<[u8; 20], Peer>,
    info_hash: [u8; 20],
    leechers: usize,
    seeders: usize,
    name: String,
    size: u64,
}

#[derive(Debug, PartialEq, Eq, Clone)]
struct Peer {
    last_announce: DateTime<Utc>,
    peer_id: [u8; 20],
    state: PeerState,
    ip: IpAddr,
    port: u16,
}

#[derive(Debug, PartialEq, Eq, Clone)]
enum PeerState {
    Leeching,
    Seeding,
}
