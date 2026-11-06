use crate::configuration::AppConfiguration;
use crate::interface::admin::{
    change_user_role, delete_user_confirm, delete_user_submit, new_user_form, new_user_submit, reset_user_password, settings,
};
use crate::interface::auth::{change_password, change_password_submit, login_submit, logout, profile};
use crate::interface::feed::torrent_feed;
use crate::interface::torrents::{delete_torrent_confirm, delete_torrent_submit, download, upload, upload_submit};
use crate::interface::{
    auth::{login, reset_password, reset_password_link, reset_password_submit},
    torrents::{list, torrent},
};

use rocket::fairing::AdHoc;
use rocket::http::Status;
use rocket::request::{FromRequest, Outcome};
use rocket_dyn_templates::{
    Template,
    handlebars::{Context, Handlebars, Helper, HelperResult, Output, RenderContext},
};

pub(crate) mod admin;
pub(crate) mod auth;
pub(crate) mod feed;
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
            .attach(AdHoc::on_response("security-headers", |_, response| {
                Box::pin(async move {
                    response.set_raw_header("Content-Security-Policy", csp());
                    response.set_raw_header("X-Frame-Options", "DENY");
                    response.set_raw_header("X-Content-Type-Options", "nosniff");
                    response.set_raw_header("Strict-Transport-Security", "max-age=31536000; includeSubDomains");
                    response.set_raw_header("Referrer-Policy", "strict-origin-when-cross-origin");
                    response.set_raw_header("Permissions-Policy", "camera=(), microphone=(), geolocation=()");
                })
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
                    reset_user_password,
                    delete_user_submit,
                    change_user_role,
                    change_password,
                    new_user_submit,
                    reset_password,
                    upload_submit,
                    new_user_form,
                    login_submit,
                    torrent_feed,
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

pub(crate) struct CsrfGuard;

#[rocket::async_trait]
impl<'r> FromRequest<'r> for CsrfGuard {
    type Error = ();

    async fn from_request(request: &'r rocket::Request<'_>) -> Outcome<Self, ()> {
        let sec_fetch_site = request.headers().get_one("Sec-Fetch-Site");
        if matches!(sec_fetch_site, Some("cross-site")) {
            return Outcome::Error((Status::Forbidden, ()));
        }

        Outcome::Success(CsrfGuard)
    }
}

fn csp() -> &'static str {
    "default-src 'self'; \
     script-src 'self' 'unsafe-inline' cdn.jsdelivr.net; \
     style-src 'self' 'unsafe-inline' cdn.jsdelivr.net; \
     img-src 'self' data:; \
     font-src 'self'; \
     frame-ancestors 'none'; \
     base-uri 'self'; \
     form-action 'self'"
}
