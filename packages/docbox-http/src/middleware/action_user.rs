//! Extractor for getting the user details from the headers set by the API

use crate::error::{DynHttpError, HttpCommonError, HttpError};
use axum::{
    extract::FromRequestParts,
    http::{StatusCode, request::Parts},
};
use docbox_core::database::{DbExecutor, models::user::User};
use thiserror::Error;
use utoipa::IntoParams;

pub struct ActionUser(pub Option<ActionUserData>);

impl ActionUser {
    /// Stores the current user details providing back the user ID to use
    pub async fn store_user(self, db: impl DbExecutor<'_>) -> Result<Option<User>, DynHttpError> {
        let user_data = match self.0 {
            Some(value) => value,
            None => return Ok(None),
        };

        let user = match User::store(db, user_data.id, user_data.name, user_data.image_id).await {
            Ok(value) => value,
            Err(error) => {
                tracing::error!(?error, "failed to store user");
                return Err(HttpCommonError::ServerError.into());
            }
        };

        Ok(Some(user))
    }
}

pub struct ActionUserData {
    pub id: String,
    pub name: Option<String>,
    pub image_id: Option<String>,
}

pub const USER_ID_HEADER: &str = "x-user-id";
pub const USER_NAME_HEADER: &str = "x-user-name";
pub const USER_IMAGE_ID_HEADER: &str = "x-user-image-id";

/// OpenAPI param for optional the user identifying headers
#[derive(IntoParams)]
#[into_params(parameter_in = Header)]
#[allow(unused)]
pub struct UserParams {
    /// Optional ID of the user if performed on behalf of a user
    #[param(rename = "x-user-id")]
    pub user_id: Option<String>,
    /// Optional name of the user if performed on behalf of a user
    #[param(rename = "x-user-name")]
    pub user_name: Option<String>,
    /// Optional image ID of the user if performed on behalf of a user
    #[param(rename = "x-user-image-id")]
    pub user_image_id: Option<String>,
}

#[derive(Debug, Error)]
#[error("user id was not a valid utf8 string")]
struct InvalidUserId;

impl HttpError for InvalidUserId {
    fn status(&self) -> axum::http::StatusCode {
        StatusCode::BAD_REQUEST
    }
}

impl<S> FromRequestParts<S> for ActionUser
where
    S: Send + Sync,
{
    type Rejection = DynHttpError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let id = match parts.headers.get(USER_ID_HEADER) {
            Some(value) => {
                let value_str = value.to_str().map_err(|_| InvalidUserId)?;
                value_str.to_string()
            }

            // Not acting on behalf of a user
            None => return Ok(ActionUser(None)),
        };

        let name = parts
            .headers
            .get(USER_NAME_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        let image_id = parts
            .headers
            .get(USER_IMAGE_ID_HEADER)
            .and_then(|value| value.to_str().ok())
            .map(|value| value.to_string());

        Ok(ActionUser(Some(ActionUserData { id, name, image_id })))
    }
}
