use password_auth::generate_hash;
use rand::{RngExt, distr::Alphanumeric};
use rocket::{
    form::Form,
    http::{Cookie, CookieJar, Status},
    request::{FromRequest, Outcome, Request},
    response::Redirect,
};
use rocket_db_pools::Connection;
use rocket_dyn_templates::{Template, context};
use serde::Serialize;
use sqlx::{prelude::FromRow, query};
use uuid::Uuid;

use crate::{Storage, interface::Either};

const SESSION_KEY: &str = "session-token";
const COOKIE_KEY: &str = "reset-token";

#[get("/login")]
pub(crate) async fn login() -> Template {
    Template::render("login", context! {})
}

#[derive(FromForm, Debug, Clone, PartialEq, Eq)]
pub(crate) struct LoginForm {
    #[field(name = "username")]
    password: String,
    #[field(name = "password")]
    confirm: String,
}

#[post("/login", data = "<form>")]
pub(crate) async fn login_submit(
    mut db: Connection<Storage>,
    cookie: &CookieJar<'_>,
    form: Form<LoginForm>,
) -> Result<Redirect, (Status, Option<Template>)> {
    let user = sqlx::query!(
        "SELECT [id] as \"uuid: Uuid\", [password] FROM [user] WHERE [username] = ? LIMIT 1",
        form.password
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|_| (Status::InternalServerError, None))?
    .ok_or((
        Status::Unauthorized,
        Some(Template::render("login", context! {error: "invalid username or password"})),
    ))?;

    if password_auth::verify_password(&form.confirm, &user.password.unwrap()).is_err() {
        return Err((
            Status::Unauthorized,
            Some(Template::render("login", context! {error: "invalid username or password"})),
        ));
    }

    let token = create_session(db, user.uuid)
        .await
        .map_err(|_| (Status::InternalServerError, None))?;

    cookie.add(Cookie::new(SESSION_KEY, token));
    Ok(Redirect::to(uri!(profile)))
}

#[post("/logout")]
pub(crate) async fn logout(mut db: Connection<Storage>, cookie: &CookieJar<'_>, user: UserSession) -> Result<Template, Status> {
    sqlx::query!("DELETE FROM [session] WHERE [token] = ?", user.session)
        .execute(&mut **db)
        .await
        .map_err(|_| Status::InternalServerError)?;

    cookie.remove(SESSION_KEY);
    Ok(Template::render("logout", context! {}))
}

#[get("/profile")]
pub(crate) async fn profile(user: UserSession) -> Template {
    Template::render("profile", context! {user})
}

#[get("/reset-password")]
pub(crate) async fn password_reset(
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

    Ok(Template::render("password", context! { username }))
}

#[derive(FromForm, Debug, Clone, PartialEq, Eq)]
pub(crate) struct PasswordForm {
    #[field(name = "password")]
    password: String,
    #[field(name = "confirm")]
    confirm: String,
}

#[post("/reset-password", data = "<form>")]
pub(crate) async fn password_submit(
    mut db: Connection<Storage>,
    cookie: &CookieJar<'_>,
    form: Form<PasswordForm>,
) -> Result<Redirect, (Status, Option<Either<Template, &'static str>>)> {
    let token = cookie
        .get(COOKIE_KEY)
        .ok_or((Status::BadRequest, Some(Either::Right("password reset token is invalid"))))?
        .value();

    let user = sqlx::query!(
        "SELECT [id] as \"uuid: Uuid\", [username] FROM [user] WHERE [reset_token] = ? AND [reset_token_expiry] > CURRENT_TIMESTAMP LIMIT 1",
        token
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|_| (Status::InternalServerError, None))?
    .ok_or((Status::Gone, Some(Either::Right("password reset token is invalid"))))?;

    if form.password != form.confirm {
        return Err((
            Status::BadRequest,
            Some(Either::Left(Template::render(
                "password",
                context! { username: user.username, error: "Passwords do not match" },
            ))),
        ));
    }

    if form.password.len() < 6 {
        return Err((
            Status::BadRequest,
            Some(Either::Left(Template::render(
                "password",
                context! { username: user.username, error: "Password must be at least 8 characters long" },
            ))),
        ));
    }

    let hashed = generate_hash(&form.password);

    sqlx::query!(
        "UPDATE [user] SET [password] = ?, [reset_token] = NULL, [reset_token_expiry] = NULL WHERE [id] = ?",
        hashed,
        user.uuid
    )
    .execute(&mut **db)
    .await
    .map_err(|_| (Status::InternalServerError, None))?;

    cookie.add(Cookie::new(
        SESSION_KEY,
        create_session(db, user.uuid)
            .await
            .map_err(|_| (Status::InternalServerError, None))?,
    ));

    Ok(Redirect::to(uri!(profile)))
}

#[get("/reset-password/<token>")]
pub(crate) async fn password_link(token: &str, cookies: &CookieJar<'_>) -> Redirect {
    cookies.add(Cookie::new(COOKIE_KEY, token.to_string()));
    Redirect::to(uri!(password_reset))
}

fn generate_token() -> String {
    rand::rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect::<String>()
}

pub(crate) async fn create_session(mut db: Connection<Storage>, user_id: Uuid) -> Result<String, sqlx::Error> {
    let token = generate_token();

    query!(
        "INSERT INTO [session] ([token], [user_id], [expiry]) VALUES (?, ?, DATETIME('now', '+1 hour'))",
        token,
        user_id
    )
    .execute(&mut **db)
    .await?;

    Ok(token)
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize)]
pub(crate) struct UserSession {
    id: Uuid,
    session: String,
    username: String,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for UserSession {
    type Error = Option<Redirect>;

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let cookies = req.cookies();
        let token = match cookies.get(SESSION_KEY) {
            None => return Outcome::Error((Status::Unauthorized, Some(Redirect::to(uri!(login))))),
            Some(cookie) => cookie.value(),
        };

        let mut db = match req.guard::<Connection<Storage>>().await {
            Outcome::Forward(_) => return Outcome::Error((Status::InternalServerError, None)),
            Outcome::Error(_) => return Outcome::Error((Status::InternalServerError, None)),
            Outcome::Success(db) => db,
        };

        let mut user = match sqlx::query_as!(
            UserSession,
            "SELECT [user].[id] as \"id: Uuid\", [user].[username], [session].[token] as [session] FROM [user] INNER JOIN [session] ON [session].[user_id] = [user].[id] WHERE [session].[token] = ? AND [session].[expiry] > CURRENT_TIMESTAMP LIMIT 1",
            token
        )
        .fetch_optional(&mut **db)
        .await
        {
            Ok(Some(user)) => user,
            Ok(None) => return Outcome::Error((Status::Unauthorized, Some(Redirect::to(uri!(login))))),
            Err(_) => return Outcome::Error((Status::InternalServerError, None)),
        };

        user.session = token.to_string();
        Outcome::Success(user)
    }
}

impl Storage {
    pub(crate) async fn reset_password(&self, user_id: Uuid) -> Result<String, sqlx::Error> {
        let token = generate_token();

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
        let token = generate_token();
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
