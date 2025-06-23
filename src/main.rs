#![feature(duration_constructors_lite)]

mod resources;
mod error;
mod oauth;
mod index;
mod db;
mod auth;
mod app;

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

#[derive(Debug, Serialize, Deserialize)]
pub struct DatabaseIndex {
    pub databases: Vec<Database>,
    pub apps: Vec<Application>,
    pub users: Vec<User>,
    pub oauth_settings: OAuthSettings,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct Database {
    pub name: String,
    pub id: DatabaseID,
    pub ro: Vec<UserID>,
    pub rw: Vec<UserID>,
    pub apps: Vec<AppID>,
    pub pages: Vec<Page>,
    pub root: PathBuf,
    pub owner: UserID,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Page {
    pub name: String,
    pub content: PathBuf,
    pub type_hint: String,
}
#[derive(Debug, Serialize, Deserialize)]
pub struct Application {
    pub name: String,
    pub id: AppID,
    pub owner: UserID,
    pub token: Token,
}
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct User {
    pub oauth: Vec<Token>,
    pub api: Vec<Token>,
    pub id: UserID,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthSettings {
    pub client_id: String,
    pub client_secret: String,
    pub redirect: String,
    pub authorisation: String,
    pub token: String,
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
    let mut db = DatabaseIndex {
        databases: vec![],
        apps: vec![],
        users: vec![],
        oauth_settings: OAuthSettings {
            client_id: "".to_string(),
            client_secret: "".to_string(),
            redirect: "".to_string(),
            authorisation: "".to_string(),
            token: "".to_string(),
        },
    };

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

    let addr = args.address;

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(oauth_settings.clone()))
            .app_data(web::Data::new(reqwest::Client::new()))
            .app_data(web::Data::new(db.clone()))
            .app_data(web::Data::new(args.clone()))
            .service(oauth::oauth)
            .service(oauth::refresh_token)
            .service(oauth::get_oauth_details)
            .service(resources::get_databases)
            .service(resources::create_database)
    })
        .workers(1)
        .bind(addr)?
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
