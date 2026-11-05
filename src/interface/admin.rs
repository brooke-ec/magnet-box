use crate::{
    Storage,
    configuration::AppConfiguration,
    interface::auth::{Admin, User, create_reset_password, create_user, get_username},
};
use rocket::{State, form::Form, http::Status};
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
    .map(|u| UserListContext {
        delete_uri: uri!(delete_user_confirm(u.id.to_string())).to_string(),
        reset_uri: uri!(reset_user_password(u.id.to_string())).to_string(),
        role_uri: uri!(change_user_role(u.id.to_string())).to_string(),
        other: user.id == u.id,
        user: u,
    })
    .collect::<Vec<_>>();

    Ok(Template::render("admin", context! { user, users }))
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
struct UserListContext {
    delete_uri: String,
    reset_uri: String,
    role_uri: String,
    other: bool,
    #[serde(flatten)]
    user: User,
}

#[get("/admin/<id>/delete")]
pub(crate) async fn delete_user_confirm(id: &str, user: Admin, mut db: Connection<Storage>) -> Result<Template, Status> {
    let id = Uuid::parse_str(id).map_err(|_| Status::NotFound)?;
    let username = get_username(&mut **db, id)
        .await
        .map_err(|e| {
            rocket::error!("Failed to fetch user from database: {}", e);
            Status::InternalServerError
        })?
        .ok_or(Status::NotFound)?;

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

#[get("/admin/<id>/reset-password")]
pub(crate) async fn reset_user_password(
    id: &str,
    user: Admin,
    mut db: Connection<Storage>,
    config: &State<AppConfiguration>,
) -> Result<Template, Status> {
    let id = Uuid::parse_str(id).map_err(|_| Status::NotFound)?;

    let token = create_reset_password(&mut **db, id).await.map_err(|e| {
        rocket::error!("Failed to reset user password: {}", e);
        Status::InternalServerError
    })?;

    let url = format!("{}{}", config.url, uri!(crate::interface::auth::reset_password_link(token)));
    Ok(Template::render("display-reset-link", context! { user, url }))
}

#[derive(FromForm, Debug, Clone, PartialEq, Eq)]
pub(crate) struct ChangeUserRoleForm {
    pub(crate) is_admin: bool,
}

#[post("/admin/<id>/role", data = "<form>")]
pub(crate) async fn change_user_role(
    id: &str,
    user: Admin,
    mut db: Connection<Storage>,
    form: Form<ChangeUserRoleForm>,
) -> Result<Template, Status> {
    let id = Uuid::parse_str(id).map_err(|_| Status::NotFound)?;

    sqlx::query!("UPDATE [user] SET [is_admin] = ? WHERE [id] = ?", form.is_admin, id)
        .execute(&mut **db)
        .await
        .map_err(|e| {
            rocket::error!("Failed to change user role in database: {}", e);
            Status::InternalServerError
        })?;

    Ok(Template::render(
        "message",
        context! { user, message: "🔑 User role updated successfully." },
    ))
}

#[get("/admin/new")]
pub(crate) async fn new_user_form(user: Admin) -> Result<Template, Status> {
    Ok(Template::render("new-user", context! { user }))
}

#[derive(FromForm, Debug, Clone, PartialEq, Eq)]
pub(crate) struct NewUserForm {
    pub(crate) username: String,
    pub(crate) admin: bool,
}

#[post("/admin/new", data = "<form>")]
pub(crate) async fn new_user_submit(
    user: Admin,
    mut db: Connection<Storage>,
    form: Form<NewUserForm>,
    config: &State<AppConfiguration>,
) -> Result<Template, Status> {
    let token = create_user(&mut **db, &form.username, form.admin).await.map_err(|e| {
        rocket::error!("Failed to create new user: {}", e);
        Status::InternalServerError
    })?;

    let url = format!("{}{}", config.url, uri!(crate::interface::auth::reset_password_link(token)));
    Ok(Template::render("display-reset-link", context! { user, url }))
}
