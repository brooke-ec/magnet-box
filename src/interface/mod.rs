use rocket_dyn_templates::{Template, context};

/// Returns a fairing that mounts the frontend endpoints.
pub(crate) fn faring() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("interface", |rocket| async { rocket.mount("/", routes![index]) })
}

#[get("/")]
pub(crate) fn index() -> Template {
    Template::render("index", context! { field: "value" })
}
