use rocket::{
    State,
    http::{ContentType, Status},
};
use rss::{
    ChannelBuilder, EnclosureBuilder, GuidBuilder, Item, ItemBuilder,
    extension::atom::{AtomExtension, Link},
};

use crate::{configuration::AppConfiguration, interface::torrents::torrent_filename, tracking::TorrentLibrary};

#[get("/feed.xml")]
pub(crate) async fn torrent_feed(
    library: &State<TorrentLibrary>,
    config: &State<AppConfiguration>,
) -> Result<(ContentType, String), Status> {
    let url = format!("{}{}", config.url, uri!(torrent_feed()).to_string());
    let mut items: Vec<Item> = Vec::with_capacity(library.len());

    library
        .iter_torrents(|torrent| {
            let download = format!(
                "{}{}",
                config.url,
                uri!(crate::interface::torrents::download_torrent(
                    torrent.id.to_string(),
                    torrent_filename(torrent.name.clone())
                ))
            );

            items.push(
                ItemBuilder::default()
                    .link(download.clone())
                    .title(torrent.name.clone())
                    .pub_date(torrent.date.to_rfc2822())
                    .guid(GuidBuilder::default().value(torrent.id).permalink(false).build())
                    .enclosure(
                        EnclosureBuilder::default()
                            .mime_type("application/x-bittorrent")
                            .length(torrent.size.to_string())
                            .url(download)
                            .build(),
                    )
                    .build(),
            );
            true
        })
        .await;

    let channel = ChannelBuilder::default()
        .title(format!("{} Torrent Feed", config.instance_title))
        .link(url.clone())
        .description(format!("A feed of all torrents uploaded to {}", config.instance_title))
        .atom_ext(AtomExtension {
            links: vec![Link {
                href: url.clone(),
                rel: "self".to_string(),
                mime_type: Some("application/rss+xml".to_string()),
                ..Default::default()
            }],
        })
        .items(items)
        .build();

    Ok((ContentType::new("application", "rss+xml"), channel.to_string()))
}
