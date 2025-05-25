#![feature(duration_constructors)]

mod database;
mod error;
mod oauth;
mod index;
mod authenticator;

use crate::error::*;
use actix_web::dev::{Payload, Service, ServiceRequest};
use actix_web::http::StatusCode;
use actix_web::{middleware, web, HttpMessage};
use actix_web::App;
use actix_web::Error;
use actix_web::FromRequest;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::HttpServer;
use base64::Engine;
use chrono::DateTime;
use chrono::Utc;
use clap::Parser;
use futures::future::{BoxFuture, LocalBoxFuture};
use futures::FutureExt;
use rand::TryRngCore;
use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use std::net::SocketAddr;
use std::ops::Deref;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;
use std::sync::MutexGuard;
use std::sync::OnceLock;
use std::task::{Context, Poll};
use actix_web::error::HttpError;
use actix_web::http::header::HeaderValue;
use futures::stream::LocalBoxStream;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;

#[derive(clap::Parser, Clone)]
struct Args {
    #[clap(default_value = "0.0.0.0:2003")]
    address: SocketAddr,

    #[clap(long = "database")]
    database_dir: PathBuf,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct DatabaseIndex {
    databases: Vec<Database>,
    apps: Vec<Application>,
    users: Vec<User>,
    oauth_settings: OAuthSettings,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Database {
    name: String,
    id: DatabaseID,
    ro: Vec<UserID>,
    rw: Vec<UserID>,
    apps: Vec<AppID>,
    pages: Vec<Page>,
    owner: UserID,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Page {
    name: String,
    content: PathBuf,
    type_hint: String,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Application {
    name: String,
    id: AppID,
    owner: UserID,
    token: String,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct User {
    oauth: Vec<Token>,
    api: Vec<Token>,
    id: UserID,
}
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct OAuthSettings {
    client_id: String,
    client_secret: String,
    redirect: String,
    authorisation: String,
    token: String,
}
pub type DatabaseID = String;
pub type UserID = String;
pub type AppID = String;

static RNG: LazyLock<Mutex<rand::rngs::OsRng>> = LazyLock::new(|| Mutex::new(rand::rngs::OsRng));

#[derive(Clone)]
pub struct DBIndex(Arc<Mutex<DatabaseIndex>>);

impl Deref for DBIndex {
    type Target = Arc<Mutex<DatabaseIndex>>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DBIndex {
    pub async fn serialise(&self) -> serde_json::Result<String> {
        let db = self.lock().await;
        serde_json::to_string_pretty(&db.deref())
    }
}

#[actix_web::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Args::parse();
    let mut db = DatabaseIndex::default();

    let index = args.database_dir.join("index.json");

    if !index.exists() {
        tokio::fs::create_dir_all(&args.database_dir).await?;
        tokio::fs::write(&index, serde_json::to_string_pretty(&db)?).await?;
    } else {
        let data = tokio::fs::read_to_string(&args.database_dir.join("index.json")).await?;
        db = serde_json::from_str(&data)?;
    }

    let oauth_settings = db.oauth_settings.clone();
    let db = DBIndex(Arc::new(Mutex::new(db)));
    index::handle_changes(args.clone(), db.clone());

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(oauth_settings.clone()))
            .app_data(web::Data::new(reqwest::Client::new()))
            .app_data(web::Data::new(db.clone()))
            .service(oauth::oauth)
            .service(oauth::refresh_token)
            .service(oauth::get_oauth_details)
            .service(database::get_databases)
    })
        .workers(1)
        .bind(args.address)?
        .run()
        .await?;

    Ok(())
}

pub async fn generate_token(len: usize) -> Result<String> {
    let mut rng = RNG.lock().await;
    let mut token = vec![0; len];
    rng.try_fill_bytes(&mut token)?;
    Ok(base64::engine::general_purpose::STANDARD.encode(&token))
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Token {
    pub token: String,
    pub refresh: String,
    pub expiry: DateTime<Utc>,
}
