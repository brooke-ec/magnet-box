//! This module contains all endpoints for the BitTorrent tracker.

use std::net::IpAddr;

use rocket::{Request, futures::lock::Mutex, http::Status, response::Responder};
use serde::Serialize;

static PEERS: Mutex<Vec<u8>> = Mutex::new(vec![]);

/// Returns all routes for the tracker module.
pub(crate) fn routes() -> Vec<rocket::Route> {
    routes![announce]
}

/// The announce endpoint for the BitTorrent tracker.
#[get("/announce?<query..>")]
async fn announce<'r>(query: AnnounceQuery<'r>, ip: IpAddr) -> Bencode<AnnounceResponseCompact> {
    println!("Announce query: {:?} from {}", query, ip);
    let mut peers = PEERS.lock().await;

    match ip {
        IpAddr::V4(ip) => {
            peers.extend_from_slice(&ip.octets());
            peers.extend_from_slice(&query.port.to_be_bytes());
        }
        _ => unimplemented!("IPv6 is not supported yet"),
    };

    Bencode(AnnounceResponseCompact {
        peers: peers.clone(),
        interval: 1800,
    })
}

#[derive(Debug, PartialEq, Eq, FromForm)]
struct AnnounceQuery<'r> {
    info_hash: &'r [u8],
    peer_id: &'r str,
    ip: Option<&'r str>,
    port: u16,
    uploaded: &'r str,
    downloaded: &'r str,
    left: &'r str,
    numwant: Option<u32>,
    event: Option<Event>,
    compact: Option<u8>,
}

#[derive(Debug, PartialEq, Eq, FromFormField)]
enum Event {
    Started,
    Completed,
    Stopped,
}

#[derive(Debug, PartialEq, Eq, Serialize)]
struct AnnounceResponseCompact {
    interval: u32,
    #[serde(with = "serde_bytes")]
    peers: Vec<u8>,
}

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
