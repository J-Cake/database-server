use actix_web::{post, web, HttpRequest, HttpResponse, Responder};
use serde::Deserialize;
use serde_json::json;
use crate::app::ValidatedApp;

#[derive(Deserialize)]
pub struct DBCall {
    pub object: String,
    pub action: Action,
    pub query: String,
}

#[derive(Deserialize)]
pub enum Action {
    Create,
    Read,
    Update,
    Delete
}

#[post("/query")]
pub async fn query(req: HttpRequest, query: web::Query<DBCall>, app: ValidatedApp, input: web::Bytes) -> actix_web::Result<impl Responder> {
    let Some(Ok(db)) = req.headers().get("db")
        .map(|v| v.to_str()) else {
        return Ok(HttpResponse::BadRequest().json(json! {{
            "success": false,
            "error": "No db header"
        }}));   
    };
    
    
    
    Ok(HttpResponse::Ok()
        .json(json! {{
            "success": true
        }}))
}