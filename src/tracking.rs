//! This module contains all endpoints for the BitTorrent tracker.

use chrono::{DateTime, Utc};
use rand::seq::index::sample;
use rocket::{
    Request, State,
    http::{Status, uri::Origin},
    response::Responder,
};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

macro_rules! err {
    ($expr:expr) => {
        BencodeError { reason: $expr.to_string() }
    };
}

/// Returns all routes for the tracker module.
pub(crate) fn routes() -> Vec<rocket::Route> {
    routes![announce]
}

/// The announce endpoint for the BitTorrent tracker.
#[get("/announce")]
async fn announce<'r>(uri: &'r Origin<'r>, ip: IpAddr, library: &State<TorrentLibrary>) -> BencodeResult<Bencode<AnnounceResponse>> {
    let query: AnnounceQuery = serde_querystring::from_bytes(
        uri.query().ok_or(err!("Invalid parameters"))?.as_bytes(),
        serde_querystring::ParseMode::UrlEncoded,
    )?;

    let info_hash: [u8; 20] = query.info_hash.as_ref().try_into().map_err(|_| err!("Invalid info_hash length"))?;
    let peer_id: [u8; 20] = query.peer_id.as_ref().try_into().map_err(|_| err!("Invalid peer_id length"))?;
    let mut torrent = library.0.get_async(&info_hash).await.ok_or(err!("Torrent not allowed"))?;

    let state = match query.left {
        0 => PeerState::Seeding,
        _ => PeerState::Leeching,
    };

    torrent.swarm.insert(
        peer_id,
        Peer {
            ip: query.ip.unwrap_or(ip),
            last_announce: Utc::now(),
            port: query.port,
            peer_id,
            state,
        },
    );

    // selects a random sample of peers from the swarm to return to the client (not including the requesting peer).
    let len = torrent.swarm.len();
    let numwant = query.numwant.unwrap_or(50).min(len);
    let peers = sample(&mut rand::rng(), len, numwant)
        .into_iter()
        .filter_map(|i| torrent.swarm.get_index(i).map(|(_, peer)| peer))
        .filter(|p| p.peer_id != peer_id)
        .collect::<Vec<&Peer>>();

    println!("Announce: {:?} from {}:{} ({} peers returned)", query, ip, query.port, peers.len());
    let peers = match query.compact {
        Some(0) => AnnounceResponsePeerList::wide(peers),
        _ => AnnounceResponsePeerList::compact(peers),
    };

    BencodeResult::Ok(Bencode(AnnounceResponse { interval: 1800, peers }))
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
    peers: AnnounceResponsePeerList,
    interval: u32,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
enum AnnounceResponsePeerList {
    Compact {
        #[serde(with = "serde_bytes")]
        peers: Vec<u8>,
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

        AnnounceResponsePeerList::Compact { peers: peers4, peers6 }
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

type BencodeResult<T> = Result<T, BencodeError>;

#[derive(Debug, PartialEq, Eq)]
struct Bencode<T>(T);

impl<'r, T: Serialize> Responder<'r, 'static> for Bencode<T> {
    fn respond_to(self, req: &'r Request<'_>) -> rocket::response::Result<'static> {
        let bytes = serde_bencoded::to_vec(&self.0).map_err(|e| {
            eprintln!("Failed to serialize to bencode: {}", e);
            Status::InternalServerError
        })?;

        bytes.respond_to(req)
    }
}

pub(crate) struct TorrentLibrary(scc::HashMap<[u8; 20], Torrent>);

impl TorrentLibrary {
    pub(crate) fn new() -> Self {
        let map = scc::HashMap::new();
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
            },
        )
        .ok();

        TorrentLibrary(map)
    }
}

#[derive(Debug, PartialEq, Eq)]
struct Torrent {
    created_at: chrono::DateTime<chrono::Utc>,
    swarm: indexmap::IndexMap<[u8; 20], Peer>,
    info_hash: [u8; 20],
    name: String,
    size: u64,
}

#[derive(Debug, PartialEq, Eq)]
struct Peer {
    last_announce: DateTime<Utc>,
    peer_id: [u8; 20],
    state: PeerState,
    ip: IpAddr,
    port: u16,
}

#[derive(Debug, PartialEq, Eq)]
enum PeerState {
    Leeching,
    Seeding,
}
