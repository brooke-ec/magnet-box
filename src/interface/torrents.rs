use humansize::{BINARY, format_size};
use rocket::State;
use rocket_dyn_templates::{Template, context};
use serde::Serialize;

use crate::{interface::auth::UserSession, tracking::TorrentLibrary};

#[get("/")]
pub(super) async fn list(library: &State<TorrentLibrary>, user: Option<UserSession>) -> Template {
    let mut torrents = Vec::<TorrentContext>::with_capacity(library.len());
    library
        .iter_torrents(|torrent| {
            torrents.push(TorrentContext {
                date: torrent.date.format("%Y-%m-%d").to_string(),
                download: format!("/download/{}", torrent.id),
                size: format_size(torrent.size, BINARY),
                url: format!("/torrent/{}", torrent.id),
                name: torrent.name.clone(),
                magnet: "".to_string(),
            });

            true
        })
        .await;

    Template::render("index", context! { user, torrents })
}

#[derive(Serialize)]
struct TorrentContext {
    download: String,
    magnet: String,
    name: String,
    size: String,
    date: String,
    url: String,
}

#[get("/upload")]
pub(crate) async fn upload(user: UserSession) -> Template {
    Template::render("upload", context! { user })
}

#[get("/torrent/<id>")]
pub(crate) async fn torrent(id: &str, library: &State<TorrentLibrary>, user: Option<UserSession>) -> Template {
    Template::render("torrent", context! { user })
}
