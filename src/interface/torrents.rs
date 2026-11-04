use std::{collections::HashSet, ops::Deref, str::FromStr};

use hightorrent::Tracker;
use humansize::{BINARY, format_size};
use rocket::{
    State,
    data::Limits,
    form::{DataField, Error, Errors, Form, FromFormField, ValueField},
    http::{ContentType, Status},
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
                download: uri!(download(format!("{}.torrent", torrent.id))).to_string(),
                date: torrent.date.format("%Y-%m-%d").to_string(),
                size: format_size(torrent.size, BINARY),
                url: format!("/torrent/{}", torrent.id),
                magnet: torrent.magnet.clone(),
                name: torrent.name.clone(),
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
        hightorrent::InfoHash::Hybrid((v1, v2)) => (Some(hex::decode(v1).unwrap()), Some(hex::decode(v2).unwrap())),
        hightorrent::InfoHash::V1(v1) => (Some(hex::decode(v1).unwrap()), None),
        hightorrent::InfoHash::V2(v2) => (None, Some(hex::decode(v2).unwrap())),
    };

    let magnet = torrent
        .magnet_link()
        .map_err(|e| {
            rocket::error!("Failed to generate magnet link: {:?}", e);
            Either::Right(Status::InternalServerError)
        })?
        .to_string();

    let size = torrent.decoded.files().iter().flatten().map(|f| f.size).sum::<u64>() as i64;
    let date = chrono::Utc::now();
    let bytes = torrent.to_vec();
    let id = Uuid::new_v4();

    sqlx::query!(
        "INSERT INTO [torrent] 
        ([id], [v1_hash], [v2_hash], [name], [size], [metainfo], [author], [date], [magnet])
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        id,
        v1_hash,
        v2_hash,
        torrent.name,
        size,
        bytes,
        user.id,
        date,
        magnet
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

    let mut files_query = sqlx::QueryBuilder::<sqlx::Sqlite>::new("INSERT INTO [file] ([torrent], [path], [size]) VALUES ");
    let mut first = false;

    for file in torrent.decoded.files().iter().flatten() {
        if !first {
            first = true;
        } else {
            files_query.push(", ");
        }

        files_query
            .push("(")
            .push_bind(id)
            .push(", ")
            .push_bind(file.path.to_string_lossy().into_owned())
            .push(", ")
            .push_bind(file.size as i64)
            .push(")");
    }

    files_query.build().execute(&mut **db).await.map_err(|e| {
        rocket::error!("Failed to insert files into database: {:?}", e);
        Either::Right(Status::InternalServerError)
    })?;

    let v1_hash = v1_hash.map(|h| InfoHash::try_from(&h[..20]).unwrap());
    let v2_hash = v2_hash.map(|h| InfoHash::try_from(&h[..20]).unwrap());

    library
        .add(id, v1_hash, v2_hash, torrent.name.clone(), size as u64, date, magnet)
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

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
struct FileTreeItem {
    indent: String,
    is_file: bool,
    name: String,
    size: String,
}

#[get("/torrent/<id>")]
pub(crate) async fn torrent(
    id: &str,
    library: &State<TorrentLibrary>,
    user: Option<UserSession>,
    mut db: Connection<Storage>,
) -> Result<Template, Status> {
    let id = Uuid::parse_str(id).map_err(|_| Status::NotFound)?;
    let torrent = library.get_torrent(&id).await.ok_or(Status::NotFound)?;

    let files = sqlx::query!("SELECT [path], [size] FROM [file] WHERE [torrent] = ? ORDER BY [path]", id)
        .fetch_all(&mut **db)
        .await
        .map_err(|e| {
            rocket::error!("Failed to fetch files for torrent {}: {:?}", id, e);
            Status::InternalServerError
        })?;

    let mut folders = HashSet::<String>::new();
    let mut tree = Vec::<FileTreeItem>::new();

    for file in files {
        let parts = file.path.split('/').collect::<Vec<_>>();
        for (depth, part) in parts.iter().enumerate() {
            let is_last = depth == parts.len() - 1;

            if is_last {
                tree.push(FileTreeItem {
                    size: format_size(file.size as u64, BINARY),
                    indent: "  ".repeat(depth),
                    name: part.to_string(),
                    is_file: true,
                });
            } else {
                let dir_path = parts[..=depth].join("/");
                if folders.insert(dir_path) {
                    tree.push(FileTreeItem {
                        indent: "  ".repeat(depth),
                        name: part.to_string(),
                        size: String::new(),
                        is_file: false,
                    });
                }
            }
        }
    }

    Ok(Template::render(
        "torrent",
        context! {
            user,
            tree,
            id: torrent.id,
            name: &torrent.name,
            magnet: &torrent.magnet,
            seeders: torrent.seeders,
            leechers: torrent.leechers,
            size: format_size(torrent.size, BINARY),
            date: torrent.date.format("%Y-%m-%d").to_string(),
            download: uri!(download(format!("{}.torrent", torrent.id))).to_string()
        },
    ))
}

#[get("/download/<id>")]
pub(crate) async fn download(id: &str, mut db: Connection<Storage>) -> Result<(ContentType, Vec<u8>), Status> {
    let id = Uuid::parse_str(&id[..id.len() - 8]).map_err(|_| Status::NotFound)?;
    let a = sqlx::query!("SELECT [metainfo] FROM [torrent] WHERE [id] = ?", id)
        .fetch_one(&mut **db)
        .await
        .map_err(|e| {
            rocket::error!("Failed to fetch torrent {}: {:?}", id, e);
            Status::InternalServerError
        })?;

    Ok((ContentType::new("application", "x-bittorrent"), a.metainfo))
}
