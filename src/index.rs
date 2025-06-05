use std::ops::Deref;
use std::sync::OnceLock;
use std::vec::IntoIter;
use chrono::{DateTime, Utc};
use tokio::sync::mpsc::Sender;
use crate::{Args, DBIndex, Token, UserID, User};

pub enum DBIndexChange {
    UserLogin {
        oauth_token: String,
        oauth_refresh: String,
        oauth_expiry: DateTime<Utc>,
        user: UserID,
        api_token: String,
        refresh_token: String,
        expiry: DateTime<Utc>,
    },
    InvalidateUserToken {
        token: Token,
    },
    RefreshUserToken { user: String, token: Token },
    Resync,
}

impl IntoIterator for DBIndexChange {
    type Item = DBIndexChange;
    type IntoIter = core::iter::Once<Self::Item>;

    fn into_iter(self) -> Self::IntoIter {
        std::iter::once(self)
    }
}

static CHANGE_DB_INDEX: OnceLock<Sender<DBIndexChange>> = OnceLock::new();

pub async fn push_change(change: impl IntoIterator<Item = DBIndexChange>) {
    for change in change {
        CHANGE_DB_INDEX
            .get()
            .unwrap()
            .send(change)
            .await
            .expect("Failed to send change");
    }
}

pub fn handle_changes(args: Args, db: DBIndex) {
    tokio::spawn(async move {
        let (sender, mut receiver) = tokio::sync::mpsc::channel(100);
        CHANGE_DB_INDEX.set(sender).unwrap();

        while let Some(change) = receiver.recv().await {
            let mut db = db.lock().await;

            match change {
                DBIndexChange::UserLogin {
                    oauth_token,
                    oauth_refresh,
                    oauth_expiry,
                    user,
                    api_token,
                    refresh_token,
                    expiry,
                } =>
                    if let Some(user) = db.users.iter_mut().find(|i| i.id.eq(&user)) {
                        user.oauth.push(Token {
                            token: oauth_token,
                            refresh: oauth_refresh,
                            expiry: oauth_expiry,
                        });
                        user.api.push(Token {
                            token: api_token,
                            refresh: refresh_token,
                            expiry,
                        });
                    } else {
                        db.users.push(User {
                            id: user.clone(),
                            oauth: vec![Token {
                                token: oauth_token,
                                refresh: oauth_refresh,
                                expiry: oauth_expiry,
                            }],
                            api: vec![Token {
                                token: api_token,
                                refresh: refresh_token,
                                expiry,
                            }],
                        })
                    },
                DBIndexChange::InvalidateUserToken { token } => for user in db.users
                    .iter_mut() {

                    user.api.retain(|i| !i.token.eq(&token.token));
                },
                DBIndexChange::RefreshUserToken { user: user_id, token } => if let Some(user) = db.users.iter_mut().find(|user| user.id == user_id) {
                    user.api.push(token);
                },
                DBIndexChange::Resync => ()
            }

            match serde_json::to_string_pretty(db.deref()) {
                Ok(data) =>
                    if let Err(err) = tokio::fs::write(args.database_dir.join("index.json"), data).await {
                        log::error!("Failed to write database index: {}", err);
                    },
                Err(e) => {
                    log::error!("Failed to write database index: {}", e);
                }
            }
        }
    });
}
