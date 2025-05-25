use std::ops::Deref;
use actix_web::{web, Error, FromRequest, HttpRequest, HttpResponse, ResponseError};
use actix_web::dev::Payload;
use futures::future::BoxFuture;
use futures::FutureExt;
use serde_json::json;
use crate::error::TokenError;
use crate::{DBIndex, User};

pub struct AuthenticatedUser(User);

impl Deref for AuthenticatedUser {
    type Target = User;
    
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl ResponseError for TokenError {
    fn error_response(&self) -> HttpResponse {
        HttpResponse::Unauthorized().json(json! {{
            "success": false,
            "message": match self {
                TokenError::MissingToken => "Missing token", 
                TokenError::InvalidToken => "Invalid token",
                TokenError::ExpiredToken => "Expired token",
                TokenError::NoUser => "Token does not belong to a user",
            }
        }})
    }
}

impl FromRequest for AuthenticatedUser {
    type Error = TokenError;
    type Future = BoxFuture<'static, actix_web::Result<Self, Self::Error>>;

    fn from_request(req: &HttpRequest, payload: &mut Payload) -> Self::Future {
        let token = req.headers().get("Authorization").cloned();
        let index = req.app_data::<web::Data<DBIndex>>().cloned();
        
        async move {
            let Some(Ok(token)) = token.map(|t| t.to_str().map(ToOwned::to_owned)) else {
                return Err(TokenError::MissingToken);
            };
            
            let Some(token) = token.strip_prefix("Bearer ") else {
                return Err(TokenError::InvalidToken);
            }; 
            
            let Some(index) = index else {
                panic!("No index");
            };
            
            for user in index.lock().await.users.iter() {
                if let Some(token) = user.api.iter().find(|u| u.token == token) {
                    return if token.expiry < chrono::Utc::now() {
                        Err(TokenError::ExpiredToken)
                    } else {
                        Ok(AuthenticatedUser(User {
                            oauth: user.oauth.clone(),
                            api: user.api.clone(),
                            id: user.id.clone(),
                        }))
                    }
                }
            }
            
            Err(TokenError::NoUser)
        }.boxed()
    }
}