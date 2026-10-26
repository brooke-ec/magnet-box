#[macro_use]
extern crate rocket;

mod configuration;
mod interface;
mod tracking;

use rocket::{fairing::AdHoc, fs::FileServer};
use rocket_db_pools::Database;
use rocket_dyn_templates::Template;

use crate::{configuration::AppConfiguration, tracking::TorrentLibrary};

#[derive(Database)]
#[database("storage")]
struct Storage(sqlx::SqlitePool);

async fn migrations(rocket: rocket::Rocket<rocket::Build>) -> rocket::fairing::Result {
    if let Some(connection) = Storage::fetch(&rocket) {
        match sqlx::migrate!("./migrations").run(&connection.0).await {
            Ok(_) => Ok(rocket),
            Err(e) => {
                eprintln!("Failed to run migrations: {}", e);
                Err(rocket)
            }
        }
    } else {
        Err(rocket)
    }
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    rocket::build()
        .attach(Storage::init())
        .attach(Template::fairing())
        .attach(AdHoc::try_on_ignite("migrations", migrations))
        .mount("/static", FileServer::from("static"))
        .attach(AdHoc::config::<AppConfiguration>())
        .manage(TorrentLibrary::new().await)
        .attach(interface::faring())
        .attach(tracking::faring())
        .launch()
        .await?;

    Ok(())
}
