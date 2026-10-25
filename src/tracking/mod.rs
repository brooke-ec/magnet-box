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
