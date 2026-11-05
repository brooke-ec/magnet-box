use crate::configuration::AppConfiguration;
use crate::interface::admin::{delete_user_confirm, delete_user_submit, settings};
use crate::interface::auth::{change_password, change_password_submit, login_submit, logout, profile};
use crate::interface::torrents::{delete_torrent_confirm, delete_torrent_submit, download, upload, upload_submit};
use crate::interface::{
    auth::{login, reset_password, reset_password_link, reset_password_submit},
    torrents::{list, torrent},
};

use rocket_dyn_templates::{
    Template,
    handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext},
};

pub(crate) mod admin;
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
                    delete_torrent_confirm,
                    change_password_submit,
                    delete_torrent_submit,
                    reset_password_submit,
                    reset_password_link,
                    delete_user_confirm,
                    delete_user_submit,
                    change_password,
                    reset_password,
                    upload_submit,
                    login_submit,
                    download,
                    settings,
                    torrent,
                    profile,
                    upload,
                    logout,
                    login,
                    list,
                ],
            )
    })
}

#[derive(Responder, Clone, Debug, PartialEq, Eq)]
pub(super) enum Either<L, R> {
    Left(L),
    Right(R),
}
