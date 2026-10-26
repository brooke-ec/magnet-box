//! This module contains all endpoints for the BitTorrent tracker.

use chrono::Utc;
use rocket::{fairing::AdHoc, futures::future::BoxFuture, tokio::time};
use std::time::Duration;

use crate::configuration::AppConfiguration;

mod announce;
mod library;

pub(crate) use library::TorrentLibrary;

use announce::announce_endpoint;
use library::PeerState;

/// Returns a fairing that mounts the tracker endpoints.
pub(crate) fn faring() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("tracker", |rocket| async {
        rocket
            .attach(AdHoc::on_liftoff("tracker:bookkeeping", bookkeeping))
            .mount("/", routes![announce_endpoint])
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
                        let mut updates = std::collections::HashMap::new();
                        library.iter_swarms(|swarm| {
                            let mut leechers = 0;
                            let mut seeders = 0;

                            swarm.peers.retain(|_, peer| {
                                let elapsed = now.signed_duration_since(peer.last_announce);
                                let retain = elapsed.num_seconds() < (interval as i64) * 2;

                                if !retain {
                                    match peer.state {
                                        PeerState::Leeching => leechers += 1,
                                        PeerState::Seeding => seeders += 1,
                                    }
                                }

                                retain
                            });

                            if leechers != 0 || seeders != 0 {
                                updates.entry(swarm.torrent).and_modify(|(l, s)| {
                                    *l += leechers;
                                    *s += seeders;
                                }).or_insert((leechers, seeders));
                            }

                            true
                        }).await;

                        for (torrent_id, (leechers, seeders)) in updates {
                            let mut torrent = library
                                .get_torrent(&torrent_id)
                                .await
                                .expect("Swarm exists, torrent does not");
                            torrent.leechers -= leechers;
                            torrent.seeders -= seeders;
                        }
                    }
                }
            }
        });
    })
}
