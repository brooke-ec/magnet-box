use chrono::Utc;
use rand::seq::index::sample;
use rocket::{
    Request, State,
    http::{ContentType, Status, uri::Origin},
    response::Responder,
};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;

use super::library::{Peer, PeerState, TorrentLibrary};
use crate::configuration::AppConfiguration;

macro_rules! err {
    ($expr:expr) => {
        BencodeError { reason: $expr.to_string() }
    };
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(super) struct AnnounceQuery {
    pub(super) info_hash: serde_bytes::ByteBuf,
    pub(super) peer_id: serde_bytes::ByteBuf,
    pub(super) ip: Option<IpAddr>,
    pub(super) port: u16,
    pub(super) uploaded: u64,
    pub(super) downloaded: u64,
    pub(super) left: u64,
    pub(super) numwant: Option<usize>,
    pub(super) event: Option<AnnounceEvent>,
    pub(super) compact: Option<u8>,
}

#[derive(Debug, Deserialize, PartialEq, Eq)]
pub(super) enum AnnounceEvent {
    #[serde(rename = "started")]
    Started,
    #[serde(rename = "completed")]
    Completed,
    #[serde(rename = "stopped")]
    Stopped,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct AnnounceResponse {
    #[serde(flatten)]
    pub(super) peers: AnnounceResponsePeerList,
    pub(super) interval: u64,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(super) enum AnnounceResponsePeerList {
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
pub(super) struct AnnounceResponsePeer {
    #[serde(with = "serde_bytes")]
    pub(super) peer_id: Vec<u8>,
    pub(super) ip: String,
    pub(super) port: u16,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
pub(super) struct BencodeError {
    #[serde(rename = "failure reason")]
    pub(super) reason: String,
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

pub(super) type BencodeResult<T> = Result<Bencode<T>, BencodeError>;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct Bencode<T>(T);

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

#[get("/announce")]
pub(super) async fn announce_endpoint<'r>(
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
                },
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
