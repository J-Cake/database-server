use crate::{generate_token, Token};
use crate::DBIndex;
use crate::OAuthSettings;
use actix_web::get;
use actix_web::post;
use actix_web::web;
use actix_web::HttpRequest;
use actix_web::HttpResponse;
use actix_web::Responder;
use chrono::DateTime;
use serde::Deserialize;
use serde_json::json;
use std::time::Duration;
use std::time::SystemTime;
use crate::index::{push_change, DBIndexChange};

#[derive(Debug, Deserialize)]
struct OAuthCode {
    code: String,
}

#[derive(Debug, Deserialize)]
struct OAuthResponse {
    access_token: String,
    expires_in: u64,
    refresh_token: String,
    user_id: String,
}

#[post("/oauth")]
pub async fn oauth(index: web::Data<OAuthSettings>, body: web::Json<OAuthCode>, client: web::Data<reqwest::Client>) -> actix_web::Result<impl Responder> {
    let oauth_response = client
        .post(&index.token)
        .json(&json! {{
            "grant_type": "authorization_code",
            "code": body.code,
            "client_id": index.client_id,
            "client_secret": index.client_secret,
            "redirect_uri": index.redirect
        }})
        .send()
        .await
        .map_err(|err| {
            actix_web::error::ErrorNotAcceptable(json! {{
                "success": false,
                "error": err.to_string()
            }})
        })?
        .text()
        .await
        .map(|i| {
            log::debug!("Got oauth response: {}", i);
            serde_json::from_str::<OAuthResponse>(&i).unwrap()
        })
        .map_err(|err| {
            log::error!("Failed to parse oauth response: {:?}", err);
            actix_web::error::ErrorInternalServerError(json! {{
                "success": false,
                "error": err.to_string()
            }})
        })?;

    let (token, refresh) = futures::future::join(generate_token(64), generate_token(128)).await;

    let token = token.map_err(|err| {
        actix_web::error::ErrorInternalServerError(json! {{
            "success": false,
            "error": err.to_string()
        }})
    })?;
    let refresh = refresh.map_err(|err| {
        actix_web::error::ErrorInternalServerError(json! {{
            "success": false,
            "error": err.to_string()
        }})
    })?;

    push_change(DBIndexChange::UserLogin {
        oauth_token: oauth_response.access_token.clone(),
        oauth_refresh: oauth_response.refresh_token.clone(),
        oauth_expiry: DateTime::from(SystemTime::now() + Duration::from_secs(oauth_response.expires_in)),
        user: oauth_response.user_id.clone(),
        api_token: token.clone(),
        refresh_token: refresh.clone(),
        expiry: DateTime::from(SystemTime::now() + Duration::from_hours(12)),
    })
    .await;

    Ok(web::Json(json! {{
        "success": true,
        "token": token,
        "refresh": refresh,
        "expires_in": 3600,
        "user": oauth_response.user_id,
    }}))
}

#[get("/oauth")]
pub async fn get_oauth_details(settings: web::Data<OAuthSettings>) -> actix_web::Result<impl Responder> {
    Ok(web::Json(json! {{
        "client_id": settings.client_id,
        "redirect": settings.redirect,
        "token": settings.token,
        "authorisation": settings.authorisation,
    }}))
}

#[derive(Debug, Deserialize)]
struct RefreshTokenRequest {
    refresh: String,
}

#[post("/refresh")]
pub async fn refresh_token(body: web::Json<RefreshTokenRequest>, index: web::Data<DBIndex>) -> actix_web::Result<impl Responder> {
    let index = index.lock().await;
    let Some((user, token)) = index
        .users
        .iter()
        .filter_map(|user| user.api.iter()
            .find(|token| token.refresh == body.refresh)
            .map(|token| (user, token.clone())))
        .next()
    else {
        return Ok(HttpResponse::Unauthorized().json(json! {{
            "success": false,
            "error": "Invalid refresh token"
        }}));
    };

    let (new_token, new_refresh) = match futures::future::join(generate_token(64), generate_token(128)).await {
        (Ok(token), Ok(refresh)) => (token, refresh),
        _ => return Ok(HttpResponse::InternalServerError().json(json! {{
            "success": false,
            "error": "Failed to generate new token"
        }}))
    };

    push_change([
        DBIndexChange::InvalidateUserToken {
            token: token.clone(),
        },
        DBIndexChange::RefreshUserToken {
            user: user.id.clone(),
            token: Token {
                token: new_token.clone(),
                refresh: new_refresh.clone(),
                expiry: DateTime::from(SystemTime::now() + Duration::from_hours(12)),
            }
        }
    ]).await;

    Ok(HttpResponse::Ok().json(json! {{
        "success": true,
        "token": new_token,
        "refresh": new_refresh,
        "expires_in": Duration::from_hours(12),
    }}))
}
