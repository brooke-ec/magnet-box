use std::ops::Deref;

use password_auth::generate_hash;
use rand::{RngExt, distr::Alphanumeric};
use rocket::time::Duration;
use rocket::{
    form::Form,
    http::{Cookie, CookieJar, Status},
    request::{FromRequest, Outcome, Request},
    response::Redirect,
};
use rocket_db_pools::Connection;
use rocket_dyn_templates::{Template, context};
use serde::{Deserialize, Serialize};
use sqlx::{Executor, FromRow, Sqlite, query};
use uuid::Uuid;

use crate::{Storage, interface::Either};

pub(crate) const SESSION_COOKIE: &str = "session-token";
pub(crate) const RESET_COOKIE: &str = "reset-token";
pub(crate) const RETURN_COOKIE: &str = "return-page";

#[get("/change-password")]
pub(crate) async fn change_password(user: User) -> Template {
    Template::render("change-password", context! { user })
}

#[derive(FromForm, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangePasswordForm {
    #[field(name = "current")]
    current_password: String,
    #[field(name = "new")]
    new_password: String,
    #[field(name = "confirm")]
    confirm_password: String,
}

#[post("/change-password", data = "<form>")]
pub(crate) async fn change_password_submit(
    mut db: Connection<Storage>,
    user: User,
    form: Form<ChangePasswordForm>,
) -> (Status, Option<Template>) {
    if form.new_password != form.confirm_password {
        return (
            Status::BadRequest,
            Some(Template::render(
                "change-password",
                context! { user, error: "New passwords do not match" },
            )),
        );
    }

    if form.new_password.len() < 6 {
        return (
            Status::BadRequest,
            Some(Template::render(
                "change-password",
                context! { user, error: "New password must be at least 6 characters long" },
            )),
        );
    }

    let user_record = match sqlx::query!("SELECT [password] FROM [user] WHERE [id] = ? LIMIT 1", user.id)
        .fetch_one(&mut **db)
        .await
    {
        Err(_) => return (Status::InternalServerError, None),
        Ok(record) => record,
    };

    if let Some(password) = user_record.password {
        if password_auth::verify_password(&form.current_password, &password).is_err() {
            return (
                Status::Unauthorized,
                Some(Template::render(
                    "change-password",
                    context! { user, error: "Current password is incorrect" },
                )),
            );
        }
    } else {
        return (
            Status::Unauthorized,
            Some(Template::render(
                "change-password",
                context! { user, error: "Current password is incorrect" },
            )),
        );
    }

    let hashed = generate_hash(&form.new_password);

    match sqlx::query!("UPDATE [user] SET [password] = ? WHERE [id] = ?", hashed, user.id)
        .execute(&mut **db)
        .await
    {
        Err(e) => {
            rocket::error!("Failed to update password: {:?}", e);
            return (Status::InternalServerError, None);
        }
        Ok(_) => (),
    }

    (
        Status::Ok,
        Some(Template::render(
            "message",
            context! { user, message: "✅ Your password has been changed successfully." },
        )),
    )
}

#[get("/login")]
pub(crate) async fn login(user: Option<User>) -> Template {
    Template::render("login", context! { user })
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
    user: Option<User>,
) -> Result<Redirect, (Status, Option<Template>)> {
    if user.is_some() {
        return Ok(Redirect::to(uri!(profile)));
    }

    let user = sqlx::query!(
        "SELECT [id] as \"uuid: Uuid\", [password] FROM [user] WHERE [username] = ? LIMIT 1",
        form.password
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|e| {
        rocket::error!("Failed to fetch user during login: {:?}", e);
        (Status::InternalServerError, None)
    })?
    .ok_or((
        Status::Unauthorized,
        Some(Template::render("login", context! { error: "invalid username or password" })),
    ))?;

    if let Some(password) = user.password {
        if password_auth::verify_password(&form.confirm, &password).is_err() {
            return Err((
                Status::Unauthorized,
                Some(Template::render("login", context! { error: "invalid username or password" })),
            ));
        }
    } else {
        return Err((
            Status::Unauthorized,
            Some(Template::render("login", context! { error: "invalid username or password" })),
        ));
    }

    let return_page = cookie
        .get(RETURN_COOKIE)
        .map(|c| {
            cookie.remove(RETURN_COOKIE);
            c.value().to_string()
        })
        .unwrap_or_else(|| uri!(profile).to_string());

    cookie.add_private(session_cookie(user.uuid));
    Ok(Redirect::to(return_page))
}

#[post("/logout")]
pub(crate) async fn logout(cookie: &CookieJar<'_>, user: User) -> Result<Template, Status> {
    cookie.remove_private(SESSION_COOKIE);
    Ok(Template::render(
        "message",
        context! { user, message: "👋 You have been logged out." },
    ))
}

#[get("/profile")]
pub(crate) async fn profile(user: User) -> Template {
    Template::render("profile", context! {user})
}

#[get("/reset-password")]
pub(crate) async fn reset_password(
    mut db: Connection<Storage>,
    cookie: &CookieJar<'_>,
    user: Option<User>,
) -> Result<Template, (Status, Option<&'static str>)> {
    let token = cookie
        .get(RESET_COOKIE)
        .ok_or((Status::BadRequest, Some("password reset token is invalid")))?
        .value();

    let username: String = sqlx::query!(
        "SELECT [username] FROM [user] WHERE [reset_token] = ? AND [reset_token_expiry] > CURRENT_TIMESTAMP LIMIT 1",
        token
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|e| {
        rocket::error!("Failed to fetch username for password reset: {:?}", e);
        (Status::InternalServerError, None)
    })?
    .ok_or((Status::Gone, Some("password reset token is invalid")))?
    .username;

    Ok(Template::render("reset-password", context! { user, username }))
}

#[derive(FromForm, Debug, Clone, PartialEq, Eq)]
pub(crate) struct PasswordForm {
    #[field(name = "password")]
    password: String,
    #[field(name = "confirm")]
    confirm: String,
}

#[post("/reset-password", data = "<form>")]
pub(crate) async fn reset_password_submit(
    mut db: Connection<Storage>,
    cookie: &CookieJar<'_>,
    form: Form<PasswordForm>,
    user_session: Option<User>,
) -> Result<Redirect, (Status, Option<Either<Template, &'static str>>)> {
    let token = cookie
        .get(RESET_COOKIE)
        .ok_or((Status::BadRequest, Some(Either::Right("password reset token is invalid"))))?
        .value();

    let user = sqlx::query!(
        "SELECT [id] as \"uuid: Uuid\", [username] FROM [user] WHERE [reset_token] = ? AND [reset_token_expiry] > CURRENT_TIMESTAMP LIMIT 1",
        token
    )
    .fetch_optional(&mut **db)
    .await
    .map_err(|e| {
        rocket::error!("Failed to fetch user for password reset: {:?}", e);
        (Status::InternalServerError, None)
    })?
    .ok_or((Status::Gone, Some(Either::Right("password reset token is invalid"))))?;

    if form.password != form.confirm {
        return Err((
            Status::BadRequest,
            Some(Either::Left(Template::render(
                "reset-password",
                context! { user: user_session, username: user.username, error: "Passwords do not match" },
            ))),
        ));
    }

    if form.password.len() < 6 {
        return Err((
            Status::BadRequest,
            Some(Either::Left(Template::render(
                "reset-password",
                context! { user: user_session, username: user.username, error: "Password must be at least 8 characters long" },
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
    .map_err(|e| {
        rocket::error!("Failed to update password during reset: {:?}", e);
        (Status::InternalServerError, None)
    })?;

    cookie.add_private(session_cookie(user.uuid));
    cookie.remove(RESET_COOKIE);

    Ok(Redirect::to(uri!(profile)))
}

#[get("/reset-password/<token>")]
pub(crate) async fn reset_password_link(token: &str, cookies: &CookieJar<'_>) -> Redirect {
    cookies.add(Cookie::new(RESET_COOKIE, token.to_string()));
    Redirect::to(uri!(reset_password))
}

fn generate_token() -> String {
    rand::rng().sample_iter(&Alphanumeric).take(32).map(char::from).collect::<String>()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Session {
    #[serde(rename = "exp")]
    expiry: chrono::DateTime<chrono::Utc>,
    #[serde(rename = "uid")]
    user_id: Uuid,
}

fn session_cookie(user_id: Uuid) -> Cookie<'static> {
    Cookie::build((
        SESSION_COOKIE,
        serde_json::to_string(&Session {
            expiry: chrono::Utc::now() + chrono::Duration::days(1),
            user_id,
        })
        .unwrap(),
    ))
    .max_age(Duration::days(1))
    .build()
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize)]
pub(crate) struct User {
    pub(crate) id: Uuid,
    pub(crate) username: String,
    pub(crate) is_admin: bool,
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for User {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        let cookies = req.cookies();
        let session = match cookies
            .get_private(SESSION_COOKIE)
            .and_then(|cookie| serde_json::from_str::<Session>(cookie.value()).ok())
        {
            None => return Outcome::Error((Status::Unauthorized, ())),
            Some(user_id) => user_id,
        };

        if session.expiry < chrono::Utc::now() {
            return Outcome::Error((Status::Unauthorized, ()));
        }

        let mut db = match req.guard::<Connection<Storage>>().await {
            Outcome::Forward(_) => {
                rocket::error!("Database connection type mismatch");
                return Outcome::Error((Status::InternalServerError, ()));
            }
            Outcome::Error(_) => {
                rocket::error!("Failed to acquire database connection");
                return Outcome::Error((Status::InternalServerError, ()));
            }
            Outcome::Success(db) => db,
        };

        let user = match sqlx::query_as!(
            User,
            "SELECT [id] as \"id: Uuid\", [username], [is_admin] as \"is_admin: bool\" FROM [user] WHERE [id] = ? LIMIT 1",
            session.user_id
        )
        .fetch_optional(&mut **db)
        .await
        {
            Ok(Some(user)) => user,
            Ok(None) => return Outcome::Error((Status::Unauthorized, ())),
            Err(e) => {
                rocket::error!("Failed to fetch user session: {:?}", e);
                return Outcome::Error((Status::InternalServerError, ()));
            }
        };

        Outcome::Success(user)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, FromRow, Serialize)]
pub(crate) struct Admin(User);

impl Deref for Admin {
    type Target = User;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[rocket::async_trait]
impl<'r> FromRequest<'r> for Admin {
    type Error = ();

    async fn from_request(req: &'r Request<'_>) -> Outcome<Self, Self::Error> {
        req.guard::<User>().await.and_then(|user| match user.is_admin {
            true => Outcome::Success(Admin(user)),
            false => Outcome::Error((Status::Forbidden, ())),
        })
    }
}

pub(crate) async fn create_reset_password<'c, E: Executor<'c, Database = Sqlite>>(db: E, user_id: Uuid) -> Result<String, sqlx::Error> {
    let token = generate_token();

    query!(
        "UPDATE [user] SET [reset_token] = ?, [reset_token_expiry] = DATETIME('now', '+1 hour') WHERE [id] = ?",
        token,
        user_id
    )
    .execute(db)
    .await?;

    Ok(token)
}

pub(crate) async fn create_user<'c, E: Executor<'c, Database = Sqlite>>(db: E, username: &str, admin: bool) -> Result<String, sqlx::Error> {
    let token = generate_token();
    let uuid = Uuid::new_v4();

    query!(
            "INSERT INTO [user] ([id], [username], [is_admin], [reset_token], [reset_token_expiry]) VALUES (?, ?, ?, ?, DATETIME('now', '+1 hour'))",
            uuid,
            username,
            admin,
            token
        )
        .execute(db)
        .await?;

    Ok(token)
}

pub(crate) async fn get_username<'c, E: Executor<'c, Database = Sqlite>>(db: E, id: Uuid) -> Result<Option<String>, sqlx::Error> {
    sqlx::query!("SELECT [username] FROM [user] WHERE [id] = ?", id)
        .fetch_optional(db)
        .await
        .map(|r| r.map(|o| o.username))
}
