use crate::{
    Storage,
    interface::auth::{Admin, User},
};
use rocket::http::Status;
use rocket_db_pools::Connection;
use rocket_dyn_templates::{Template, context};
use serde::Serialize;
use uuid::Uuid;

#[get("/admin")]
pub(crate) async fn settings(user: Admin, mut db: Connection<Storage>) -> Result<Template, Status> {
    let users = sqlx::query_as!(
        User,
        "SELECT [id] as \"id: uuid::Uuid\", [username], [is_admin] as \"is_admin: bool\" FROM [user]"
    )
    .fetch_all(&mut **db)
    .await
    .map_err(|e| {
        rocket::error!("Failed to fetch users from database: {}", e);
        Status::InternalServerError
    })?
    .into_iter()
    .map(|user| UserListContext {
        delete_uri: uri!(delete_user_confirm(user.id.to_string())).to_string(),
        reset_uri: "".to_string(),
        role_uri: "".to_string(),
        user,
    })
    .collect::<Vec<_>>();

    Ok(Template::render("admin", context! { user, users }))
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
struct UserListContext {
    delete_uri: String,
    reset_uri: String,
    role_uri: String,
    #[serde(flatten)]
    user: User,
}

#[get("/admin/<id>/delete")]
pub(crate) async fn delete_user_confirm(id: &str, user: Admin, mut db: Connection<Storage>) -> Result<Template, Status> {
    let id = Uuid::parse_str(id).map_err(|_| Status::NotFound)?;
    let username = sqlx::query!("SELECT [username] FROM [user] WHERE [id] = ?", id)
        .fetch_optional(&mut **db)
        .await
        .map_err(|e| {
            rocket::error!("Failed to fetch user from database: {}", e);
            Status::InternalServerError
        })?
        .ok_or(Status::NotFound)?
        .username;

    Ok(Template::render("delete-user", context! { user, username }))
}

#[post("/admin/<id>/delete")]
pub(crate) async fn delete_user_submit(id: &str, user: Admin, mut db: Connection<Storage>) -> Result<Template, Status> {
    let id = Uuid::parse_str(id).map_err(|_| Status::NotFound)?;
    sqlx::query!("DELETE FROM [user] WHERE [id] = ?", id)
        .execute(&mut **db)
        .await
        .map_err(|e| {
            rocket::error!("Failed to delete user from database: {}", e);
            Status::InternalServerError
        })?;

    Ok(Template::render(
        "message",
        context! { user, message: "🗑️ User deleted successfully." },
    ))
}
