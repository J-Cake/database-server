use std::ops::Deref;
use actix_web::{web, FromRequest, HttpRequest, HttpResponse, ResponseError};
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::json;
use crate::{Application, DBIndex, User};
use crate::auth::AuthenticatedUser;
use crate::error::{AppError, TokenError};

pub struct ValidatedApp(Application);

impl Deref for ValidatedApp {
    type Target = Application;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ResponseError for AppError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::Unauthorized().json(json! {{
            "success": false,
            "message": match self {
                AppError::MissingToken => "Missing token",
                AppError::InvalidToken => "Invalid token",
                AppError::ExpiredToken => "Expired token",
                AppError::NoApp => "Token does not belong to an application",
            }
        }})
    }
}

impl FromRequest for ValidatedApp {
    type Error = AppError;
    type Future = BoxFuture<'static, actix_web::Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, _: &mut actix_web::dev::Payload) -> Self::Future {
        let token = req.headers().get("Authorization").cloned();
        let index = req.app_data::<web::Data<DBIndex>>().cloned();
        
        async move {
            let Some(Ok(token)) = token.map(|t| t.to_str().map(ToOwned::to_owned)) else {
                return Err(AppError::MissingToken);
            };
            
            let Some(token) = token.strip_prefix("Bearer ") else {
                return Err(AppError::InvalidToken);
            }; 
            
            let Some(index) = index else {
                panic!("No index");
            };
            
            for app in index.lock().await.apps.iter() {
                if app.token.token.eq(&token) {
                    return Ok(ValidatedApp(Application {
                        name: app.name.clone(),
                        id: app.id.clone(),
                        owner: app.owner.clone(),
                        token: app.token.clone(),
                    }));
                }
            }
            
            Err(AppError::NoApp)
        }.boxed()
    }
}