use actix_web::{get, web, Responder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::{generate_token, DBIndex, DatabaseID, DatabaseIndex};
use crate::authenticator::AuthenticatedUser;

#[derive(Deserialize)]
pub struct GetDatabasesOptions {
    membership: Option<Membership>,
    name: Option<String>,
}

#[derive(Copy, Clone, Deserialize)]
pub enum Membership {
    Owner,
    ReadWrite,
    ReadOnly,
    Member
}

#[derive(Serialize, Deserialize)]
pub struct DatabaseDescription {
    name: String,
    id: DatabaseID,
    owner: String,
    rw: Vec<String>,
    ro: Vec<String>,
    objects: u64
}

#[get("/databases")]
pub async fn get_databases(query: web::Query<GetDatabasesOptions>, user: AuthenticatedUser, index: web::Data<DBIndex>) -> impl Responder {
    let databases = index.lock().await
        .databases
        .iter()
        .filter(|db| match query.membership {
            Some(Membership::Owner) => db.owner == user.id,
            Some(Membership::ReadWrite) => db.rw.contains(&user.id),
            Some(Membership::ReadOnly) => db.ro.contains(&user.id),
            Some(Membership::Member) | None => db.rw.contains(&user.id) || db.ro.contains(&user.id) || db.owner == user.id,
        })
        .filter(|db| match query.name.as_ref() {
            Some(name) => db.name.eq(name),
            None => true,
        })
        .map(|db| DatabaseDescription {
            name: db.name.clone(),
            owner: db.owner.clone(),
            rw: db.rw.clone(),
            ro: db.ro.clone(),
            objects: 0,
            id: db.id.clone()
        })
        .collect::<Vec<_>>();

    web::Json(json! {{
        "databases": databases
    }})
}