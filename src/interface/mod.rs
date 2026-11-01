use crate::interface::{
    auth::{login, password, password_link, password_submit},
    torrents::{list, torrent},
};

pub(crate) mod auth;
pub(crate) mod torrents;

/// Returns a fairing that mounts the frontend endpoints.
pub(crate) fn faring() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("interface", |rocket| async {
        rocket.mount("/", routes![list, torrent, login, password, password_link, password_submit])
    })
}
