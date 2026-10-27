use crate::interface::torrents::{list, torrent};

mod torrents;

/// Returns a fairing that mounts the frontend endpoints.
pub(crate) fn faring() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("interface", |rocket| async { rocket.mount("/", routes![list, torrent]) })
}
