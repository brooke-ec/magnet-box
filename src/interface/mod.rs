use crate::configuration::AppConfiguration;
use crate::interface::auth::{login_submit, logout, profile};
use crate::interface::{
    auth::{login, password_link, password_reset, password_submit},
    torrents::{list, torrent},
};

use rocket_dyn_templates::{
    Template,
    handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext},
};

pub(crate) mod auth;
pub(crate) mod torrents;

/// Returns a fairing that mounts the frontend endpoints.
pub(crate) fn faring() -> rocket::fairing::AdHoc {
    rocket::fairing::AdHoc::on_ignite("interface", |rocket| async {
        let instance_title = Box::leak(Box::new(
            rocket
                .state::<AppConfiguration>()
                .expect("configuration state must be attached")
                .instance_title
                .clone(),
        ))
        .as_str();

        rocket
            .attach(Template::custom(move |engines| {
                engines.handlebars.register_helper(
                    "instance_title",
                    Box::new(
                        move |_: &Helper, _: &Handlebars, _: &Context, _: &mut RenderContext, output: &mut dyn Output| -> HelperResult {
                            output.write(instance_title)?;
                            Ok(())
                        },
                    ),
                );
            }))
            .mount(
                "/",
                routes![
                    password_submit,
                    password_reset,
                    password_link,
                    login_submit,
                    torrent,
                    profile,
                    logout,
                    login,
                    list,
                ],
            )
    })
}

#[derive(Responder)]
pub(super) enum Either<L, R> {
    Left(L),
    Right(R),
}
