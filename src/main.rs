#[macro_use]
extern crate rocket;

mod configuration;
mod interface;
mod tracking;

use rocket::{Config, fairing::AdHoc, fs::FileServer};
use rocket_db_pools::Database;
use uuid::Uuid;

use crate::{configuration::AppConfiguration, tracking::TorrentLibrary};

#[derive(Database)]
#[database("storage")]
struct Storage(sqlx::SqlitePool);

async fn database_init(rocket: rocket::Rocket<rocket::Build>) -> rocket::fairing::Result {
    let connection = match Storage::fetch(&rocket) {
        Some(connection) => connection,
        None => return Err(rocket),
    };

    // run migrations
    match sqlx::migrate!("./migrations").run(&**connection).await {
        Ok(_) => (),
        Err(e) => {
            rocket::error!("Failed to run database migrations: {}", e);
            return Err(rocket);
        }
    }

    // display reset link if no user with a password exists
    if match sqlx::query!("SELECT TRUE FROM [user] WHERE [PASSWORD] IS NOT NULL LIMIT 1")
        .fetch_optional(&**connection)
        .await
    {
        Ok(option) => option.is_none(),
        Err(e) => {
            rocket::error!("Failed to query user: {}", e);
            return Err(rocket);
        }
    } {
        let user: Option<Uuid> = match sqlx::query!("SELECT [id] as \"uuid: Uuid\" FROM [user] WHERE [password] IS NULL LIMIT 1")
            .fetch_optional(&**connection)
            .await
        {
            Ok(record) => record.map(|record| record.uuid),
            Err(error) => {
                rocket::error!("Failed to query user: {}", error);
                return Err(rocket);
            }
        };

        let token = match user {
            Some(user_id) => match connection.reset_password(user_id).await {
                Ok(token) => token,
                Err(error) => {
                    rocket::error!("Failed to create reset token: {}", error);
                    return Err(rocket);
                }
            },
            None => match connection.create_user("admin").await {
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

    Ok(rocket)
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    rocket::build()
        .attach(Storage::init())
        .attach(AdHoc::try_on_ignite("migrations", database_init))
        .mount("/static", FileServer::from("static"))
        .attach(AdHoc::config::<AppConfiguration>())
        .manage(TorrentLibrary::new().await)
        .attach(interface::faring())
        .attach(tracking::faring())
        .launch()
        .await?;

    Ok(())
}
