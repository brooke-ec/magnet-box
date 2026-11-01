use rand::{RngExt, distr::Alphanumeric};
use rocket::{
    State,
    form::Form,
    http::{Cookie, CookieJar, Status},
    response::Redirect,
};
use rocket_db_pools::Connection;
use rocket_dyn_templates::{Template, context};
use sqlx::query;
use uuid::Uuid;

use crate::{Storage, configuration::AppConfiguration};

const COOKIE_KEY: &str = "reset-token";

#[get("/login")]
pub(super) async fn login(config: &State<AppConfiguration>) -> Template {
    Template::render("login", context! { instance_title: &config.instance_title })
}

#[get("/reset")]
pub(crate) async fn password(
    config: &State<AppConfiguration>,
    mut db: Connection<Storage>,
    cookie: &CookieJar<'_>,
) -> Result<Template, (Status, Option<&'static str>)> {
    let token = cookie
        .get(COOKIE_KEY)
        .ok_or((Status::BadRequest, Some("password reset token is invalid")))?
        .value();

    let username: String = sqlx::query!(
        "SELECT [username] FROM [user] WHERE [reset_token] = ? AND [reset_token_expiry] > CURRENT_TIMESTAMP LIMIT 1",
        token
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|_| (Status::InternalServerError, None))?
    .ok_or((Status::Gone, Some("password reset token is invalid")))?
    .username;

    Ok(Template::render(
        "password",
        context! { instance_title: &config.instance_title, username },
    ))
}

#[derive(FromForm)]
pub(crate) struct PasswordForm {
    password: String,
    password_confirm: String,
}

#[post("/reset", data = "<form>")]
pub(crate) fn password_submit(form: Form<PasswordForm>) -> Result<Template, (Status, Option<&'static str>)> {
    Err((Status::NotImplemented, Some("password reset submission is not implemented yet")))
}

#[get("/reset/<token>")]
pub(crate) async fn password_link(token: &str, cookies: &CookieJar<'_>) -> Redirect {
    cookies.add(Cookie::new(COOKIE_KEY, token.to_string()));
    Redirect::to(uri!(password))
}

impl Storage {
    pub(crate) async fn reset_password(&self, user_id: Uuid) -> Result<String, sqlx::Error> {
        let token: String = rand::rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect();

        query!(
            "UPDATE [user] SET [reset_token] = ?, [reset_token_expiry] = DATETIME('now', '+1 hour') WHERE [id] = ?",
            token,
            user_id
        )
        .execute(&**self)
        .await?;

        Ok(token)
    }

    pub(crate) async fn create_user(&self, username: &str) -> Result<String, sqlx::Error> {
        let token = rand::rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect::<String>();
        let uuid = Uuid::new_v4();

        query!(
            "INSERT INTO [user] ([id], [username], [reset_token], [reset_token_expiry]) VALUES (?, ?, ?, DATETIME('now', '+1 hour'))",
            uuid,
            username,
            token
        )
        .execute(&**self)
        .await?;

        Ok(token)
    }
}
