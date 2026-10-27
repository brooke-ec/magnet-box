use humansize::{BINARY, format_size};
use rocket::State;
use rocket_dyn_templates::{Template, context};
use serde::Serialize;

use crate::{configuration::AppConfiguration, tracking::TorrentLibrary};

#[get("/")]
pub(super) async fn list(library: &State<TorrentLibrary>, config: &State<AppConfiguration>) -> Template {
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

    Template::render("index", context! { instance_title: &config.instance_title, torrents  })
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

#[get("/torrent/<id>")]
pub(super) async fn torrent(id: &str, config: &State<AppConfiguration>, library: &State<TorrentLibrary>) -> Template {
    Template::render("torrent", context! { instance_title: &config.instance_title })
}
