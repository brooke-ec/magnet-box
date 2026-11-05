#[macro_use]
extern crate rocket;

mod configuration;
mod interface;
mod tracking;

use rocket::{
    Config, Request,
    fairing::AdHoc,
    fs::{FileServer, NamedFile},
    futures::future::try_join_all,
    http::Cookie,
    response::Redirect,
};
use rocket_db_pools::Database;
use uuid::Uuid;

use crate::{
    configuration::AppConfiguration,
    interface::{
        Either,
        auth::{RETURN_COOKIE, create_reset_password, create_user},
    },
    tracking::{InfoHash, TorrentLibrary},
};

#[derive(Database)]
#[database("storage")]
struct Storage(sqlx::SqlitePool);

async fn database_init(rocket: rocket::Rocket<rocket::Build>) -> rocket::fairing::Result {
    let db = match Storage::fetch(&rocket) {
        Some(connection) => connection,
        None => return Err(rocket),
    };

    // run migrations
    match sqlx::migrate!("./migrations").run(&**db).await {
        Ok(_) => (),
        Err(e) => {
            rocket::error!("Failed to run database migrations: {}", e);
            return Err(rocket);
        }
    }

    // display reset link if no user with a password exists
    if match sqlx::query!("SELECT TRUE FROM [user] WHERE [PASSWORD] IS NOT NULL LIMIT 1")
        .fetch_optional(&**db)
        .await
    {
        Ok(option) => option.is_none(),
        Err(e) => {
            rocket::error!("Failed to query user: {}", e);
            return Err(rocket);
        }
    } {
        let user: Option<Uuid> = match sqlx::query!("SELECT [id] as \"uuid: Uuid\" FROM [user] WHERE [password] IS NULL LIMIT 1")
            .fetch_optional(&**db)
            .await
        {
            Ok(record) => record.map(|record| record.uuid),
            Err(error) => {
                rocket::error!("Failed to query user: {}", error);
                return Err(rocket);
            }
        };

        let token = match user {
            Some(user_id) => match create_reset_password(&**db, user_id).await {
                Ok(token) => token,
                Err(error) => {
                    rocket::error!("Failed to create reset token: {}", error);
                    return Err(rocket);
                }
            },
            None => match create_user(&**db, "admin", true).await {
                Ok(token) => token,
                Err(error) => {
                    rocket::error!("Failed to create user: {}", error);
                    return Err(rocket);
                }
            },
        };

        rocket::warn!(
            "No user with a password was found. Set up the first user: http://127.0.0.1:{}{}",
            rocket.figment().extract::<Config>().unwrap().port,
            uri!(crate::interface::auth::reset_password_link(token))
        );
    }

    // load torrent library
    let library = match rocket.state::<TorrentLibrary>() {
        Some(library) => library,
        None => {
            rocket::error!("Failed to get torrent library state");
            return Err(rocket);
        }
    };

    match sqlx::query!(
        "SELECT
        [id] as \"id: Uuid\",
        [v1_hash],
        [v2_hash],
        [name],
        [size],
        [date],
        [magnet],
        [author] 
        FROM [torrent] ORDER BY [date] DESC"
    )
    .fetch_all(&**db)
    .await
    {
        Ok(records) => {
            match try_join_all(records.into_iter().map(async |record| {
                library
                    .add(
                        record.id,
                        record.v1_hash.map(|h| InfoHash::try_from(&h[..20]).unwrap()),
                        record.v2_hash.map(|h| InfoHash::try_from(&h[..20]).unwrap()),
                        record.name,
                        record.author,
                        record.size as u64,
                        record.date.and_utc(),
                        record.magnet,
                    )
                    .await
            }))
            .await
            {
                Ok(_) => (),
                Err(_) => {
                    rocket::error!("Failed to load torrents");
                    return Err(rocket);
                }
            }
        }
        Err(error) => {
            rocket::error!("Failed to load torrents: {}", error);
            return Err(rocket);
        }
    };

    Ok(rocket)
}

#[catch(401)]
fn unauthorized(request: &Request<'_>) -> Either<Redirect, &'static str> {
    if request.headers().get("User-Agent").any(|ua| ua.contains("Mozilla")) {
        request.cookies().add(Cookie::new(RETURN_COOKIE, request.uri().to_string()));
        Either::Left(Redirect::to(uri!(crate::interface::auth::login)))
    } else {
        Either::Right("Unauthorized")
    }
}

#[rocket::get("/favicon.ico")]
async fn favicon() -> Option<NamedFile> {
    NamedFile::open("static/favicon.ico").await.ok()
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    rocket::build()
        .attach(Storage::init())
        .attach(AdHoc::try_on_ignite("migrations", database_init))
        .mount("/static", FileServer::from("static"))
        .attach(AdHoc::config::<AppConfiguration>())
        .register("/", catchers![unauthorized])
        .mount("/", rocket::routes![favicon])
        .manage(TorrentLibrary::new().await)
        .attach(interface::faring())
        .attach(tracking::faring())
        .launch()
        .await?;

    Ok(())
}
