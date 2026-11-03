use std::{ops::Deref, str::FromStr};

use hightorrent::Tracker;
use humansize::{BINARY, format_size};
use rocket::{
    State,
    data::Limits,
    form::{DataField, Error, Errors, Form, FromFormField, ValueField},
    http::Status,
    response::Redirect,
};
use rocket_db_pools::Connection;
use rocket_dyn_templates::{Template, context};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    Storage,
    configuration::AppConfiguration,
    interface::{Either, auth::UserSession},
    tracking::{InfoHash, TorrentLibrary},
};

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

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
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

#[post("/upload", data = "<torrent>")]
pub(crate) async fn upload_submit(
    config: &State<AppConfiguration>,
    library: &State<TorrentLibrary>,
    mut db: Connection<Storage>,
    torrent: Form<TorrentForm>,
    user: UserSession,
) -> Result<Redirect, Either<Template, Status>> {
    let tracker = Tracker::from_str(&format!("{}/announce", config.url)).map_err(|e| {
        rocket::error!("Failed to parse tracker URL: {:?}", e);
        Either::Right(Status::InternalServerError)
    })?;

    if !torrent.trackers().contains(&tracker) {
        return Err(Either::Left(Template::render(
            "upload",
            context! { user, error: format!("Torrent must contain the tracker: '{}'", tracker.url()) },
        )));
    }

    let (v1_hash, v2_hash) = match &torrent.hash {
        hightorrent::InfoHash::Hybrid((v1, v2)) => (Some(v1.as_bytes()), Some(v2.as_bytes())),
        hightorrent::InfoHash::V1(v1) => (Some(v1.as_bytes()), None),
        hightorrent::InfoHash::V2(v2) => (None, Some(v2.as_bytes())),
    };

    let size = torrent.decoded.files().iter().flatten().map(|f| f.size).sum::<u64>() as i64;
    let date = chrono::Utc::now();
    let bytes = torrent.to_vec();
    let id = Uuid::new_v4();

    sqlx::query!(
        "INSERT INTO [torrent] ([id], [v1_hash], [v2_hash], [name], [size], [metainfo], [author], [date]) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        v1_hash,
        v2_hash,
        torrent.name,
        size,
        bytes,
        user.id,
        date,
    )
    .execute(&mut **db)
    .await
    .map_err(|e| {
        if e.as_database_error().is_some_and(|e| e.is_unique_violation()) {
            Either::Left(Template::render(
                "upload",
                context! { user, error: "Torrent has already been uploaded" },
            ))
        } else {
            rocket::error!("Failed to insert torrent into database: {:?}", e);
            Either::Right(Status::InternalServerError)
        }
    })?;

    let v1_hash = v1_hash.map(|h| InfoHash::try_from(&h[..20]).unwrap());
    let v2_hash = v2_hash.map(|h| InfoHash::try_from(&h[..20]).unwrap());

    library
        .add(id, v1_hash, v2_hash, torrent.name.clone(), size as u64, date)
        .await
        .map_err(|e| {
            rocket::error!("Failed to add torrent to library: {:?}", e);
            Either::Right(Status::InternalServerError)
        })?;

    Ok(Redirect::to(uri!(torrent(id.to_string()))))
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(crate) struct TorrentForm(hightorrent::TorrentFile);

impl Deref for TorrentForm {
    type Target = hightorrent::TorrentFile;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[rocket::async_trait]
impl<'v> FromFormField<'v> for TorrentForm {
    fn from_value(field: ValueField<'v>) -> rocket::form::Result<'v, Self> {
        hightorrent::TorrentFile::from_slice(field.value.as_bytes())
            .map_err(|error| Errors::from(Error::custom(error)))
            .map(|t| Self(t))
    }

    async fn from_data(field: DataField<'v, '_>) -> rocket::form::Result<'v, Self> {
        let bytes = field.data.open(Limits::FILE).into_bytes().await.map_err(Error::custom)?.value;
        hightorrent::TorrentFile::from_slice(&bytes)
            .map_err(|error| Errors::from(Error::custom(error)))
            .map(|t| Self(t))
    }
}

#[get("/torrent/<id>")]
pub(crate) async fn torrent(id: &str, library: &State<TorrentLibrary>, user: Option<UserSession>) -> Template {
    Template::render("torrent", context! { user })
}
