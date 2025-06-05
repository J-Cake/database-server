use actix_web::{get, put, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::json;
use crate::{generate_token, Args, DBIndex, Database, DatabaseID, DatabaseIndex};
use crate::auth::AuthenticatedUser;
use crate::index::{handle_changes, push_change, DBIndexChange};

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

/// TODO: Get database health - Perform an index check to see how large it is and whether it's corrupt.
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
        "success": true,
        "databases": databases,
        "health": 1,
        "objects": Option::<usize>::None
    }})
}

#[derive(Deserialize)]
pub struct CreateDBOptions {
    name: String,
    ro: Option<Vec<String>>,
    rw: Option<Vec<String>>,
}

#[put("/databases")]
pub async fn create_database(options: web::Query<CreateDBOptions>, user: AuthenticatedUser, index: web::Data<DBIndex>, args: web::Data<Args>) -> actix_web::Result<impl Responder> {
    let mut index = index.lock().await;
    let token = loop {
        let token = match generate_token(16).await {
            Ok(token) => token,
            Err(err) => return Ok(HttpResponse::InternalServerError().json(json! {{
                "success": false,
                "error": err.to_string()
            }}))
        };

        if !index.databases.iter().any(|db| db.id == token) {
            break token;
        }
    };

    let db_dir = args.database_dir.join(&token);
    tokio::fs::create_dir_all(&db_dir).await?;

    index.databases.push(Database {
        id: token.clone(),
        name: options.name.clone(),
        owner: user.id.clone(),
        rw: options.rw.clone().unwrap_or_default(),
        ro: options.ro.clone().unwrap_or_default(),
        apps: vec![],
        root: db_dir,
        pages: vec![]
    });

    push_change(DBIndexChange::Resync).await;

    Ok(HttpResponse::Created()
        .json(json! {{
            "success": true,
            "id": token.clone(),
            "name": options.name.clone()
        }}))
}