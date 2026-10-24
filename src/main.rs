#[macro_use]
extern crate rocket;

mod tracking;

use rocket_dyn_templates::{Template, context};

#[get("/")]
pub(crate) fn index() -> Template {
    Template::render("index", context! { field: "value" })
}

#[launch]
fn rocket() -> _ {
    rocket::build()
        .attach(Template::fairing())
        .mount("/", routes![index])
        .mount("/", tracking::routes())
}
