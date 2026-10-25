#[macro_use]
extern crate rocket;

mod tracking;

use rocket_db_pools::Database;
use rocket_dyn_templates::{Template, context};

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

#[get("/")]
pub(crate) fn index() -> Template {
    Template::render("index", context! { field: "value" })
}

#[main]
async fn main() -> Result<(), rocket::Error> {
    rocket::build()
        .attach(Storage::init())
        .attach(Template::fairing())
        .attach(rocket::fairing::AdHoc::try_on_ignite("migrations", migrations))
        .mount("/", routes![index])
        .mount("/", tracking::routes())
        .launch()
        .await?;

    Ok(())
}
