use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
};
use axum::{
    Json,
    extract::{FromRef, FromRequestParts, Path, State},
    http::{StatusCode, header, request::Parts},
};
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, Row, types::JsonValue};
use std::sync::Arc;
use uuid::Uuid;

use crate::AppState;
use crate::db::Db;

#[derive(FromRow, Serialize)]
pub struct User {
    pub id: Uuid,
    pub username: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    #[serde(skip_serializing)]
    pub created_at: chrono::NaiveDateTime,
}

#[derive(FromRow, Serialize)]
pub struct Role {
    pub id: Uuid,
    pub name: String,
    pub permissions: JsonValue,
}

#[derive(Deserialize)]
pub struct RegisterInput {
    pub username: String,
    pub password: String,
}

#[derive(Serialize)]
struct TokenResponse {
    token: String,
}

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterInput>,
) -> Result<Json<User>, StatusCode> {
    let hash = hash_password(&input.password)?;
    let user = sqlx::query_as::<_, User>(
        r#"INSERT INTO users (username, password_hash) VALUES ($1, $2) RETURNING id, username, password_hash, created_at"#
    )
        .bind(&input.username)
        .bind(&hash)
        .fetch_one(&state.db)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(Json(user))
}

#[derive(Deserialize)]
pub struct LoginInput {
    pub username: String,
    pub password: String,
}

#[derive(Serialize, Deserialize)]
struct Claims {
    sub: Uuid,
    exp: usize,
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginInput>,
) -> Result<Json<TokenResponse>, StatusCode> {
    let user = sqlx::query_as::<_, User>(
        r#"SELECT id, username, password_hash, created_at FROM users WHERE username = $1"#,
    )
    .bind(&input.username)
    .fetch_optional(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?
    .ok_or(StatusCode::UNAUTHORIZED)?;

    verify_password(&user.password_hash, &input.password)?;

    let claims = Claims {
        sub: user.id,
        exp: chrono::Utc::now().timestamp() as usize + 24 * 3600,
    };
    let token = jsonwebtoken::encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.jwt_secret.as_bytes()),
    )
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(Json(TokenResponse { token }))
}

// extractor for authenticated user
pub struct AuthUser {
    pub id: Uuid,
}

use async_trait::async_trait;

#[async_trait]
impl<S> FromRequestParts<S> for AuthUser
where
    Arc<AppState>: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let state: Arc<AppState> = Arc::from_ref(state);
        let auth = parts
            .headers
            .get(header::AUTHORIZATION)
            .and_then(|h| h.to_str().ok())
            .ok_or(StatusCode::UNAUTHORIZED)?;
        if !auth.starts_with("Bearer ") {
            return Err(StatusCode::UNAUTHORIZED);
        }
        let token = &auth[7..];
        let data = jsonwebtoken::decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| StatusCode::UNAUTHORIZED)?;
        Ok(AuthUser {
            id: data.claims.sub,
        })
    }
}

fn hash_password(pwd: &str) -> Result<String, StatusCode> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(pwd.as_bytes(), &salt)
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)
        .map(|h| h.to_string())
}

fn verify_password(hash: &str, pwd: &str) -> Result<(), StatusCode> {
    let parsed = PasswordHash::new(hash).map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Argon2::default()
        .verify_password(pwd.as_bytes(), &parsed)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}

// role assignment
#[derive(Deserialize)]
pub struct AssignRole {
    pub role: Uuid,
}

pub async fn assign_role(
    Path(id): Path<Uuid>,
    State(state): State<AppState>,
    AuthUser { id: current }: AuthUser,
    Json(input): Json<AssignRole>,
) -> Result<StatusCode, StatusCode> {
    if !user_has_role(&state.db, current, &["Owner", "Admin"]).await? {
        return Err(StatusCode::FORBIDDEN);
    }
    sqlx::query("INSERT INTO user_roles (user_id, role_id) VALUES ($1,$2) ON CONFLICT DO NOTHING")
        .bind(id)
        .bind(input.role)
        .execute(&state.db)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn user_has_role(db: &Db, user: Uuid, roles: &[&str]) -> Result<bool, StatusCode> {
    let exists: (bool,) = sqlx::query_as(
        "SELECT EXISTS (SELECT 1 FROM user_roles ur JOIN roles r ON ur.role_id = r.id WHERE ur.user_id = $1 AND r.name = ANY($2)) as exists"
    )
        .bind(user)
        .bind(roles)
        .fetch_one(db)
        .await
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;
    Ok(exists.0)
}

#[derive(Serialize)]
pub struct UserWithRoles {
    pub id: Uuid,
    pub username: String,
    pub roles: Vec<String>,
}

pub async fn list_users(
    State(state): State<AppState>,
) -> Result<Json<Vec<UserWithRoles>>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT u.id, u.username, r.name FROM users u
           LEFT JOIN user_roles ur ON u.id = ur.user_id
           LEFT JOIN roles r ON ur.role_id = r.id
           ORDER BY u.username"#,
    )
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let mut map: std::collections::HashMap<Uuid, UserWithRoles> = std::collections::HashMap::new();
    for row in rows {
        let id: Uuid = row.get("id");
        let entry = map.entry(id).or_insert_with(|| UserWithRoles {
            id,
            username: row.get("username"),
            roles: Vec::new(),
        });
        if let Ok(Some(role)) = row.try_get::<Option<String>, _>("name") {
            entry.roles.push(role);
        }
    }
    Ok(Json(map.into_iter().map(|(_, v)| v).collect()))
}

pub async fn me(
    State(state): State<AppState>,
    AuthUser { id }: AuthUser,
) -> Result<Json<UserWithRoles>, StatusCode> {
    let rows = sqlx::query(
        r#"SELECT u.id, u.username, r.name FROM users u
           LEFT JOIN user_roles ur ON u.id = ur.user_id
           LEFT JOIN roles r ON ur.role_id = r.id
           WHERE u.id = $1"#,
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    if rows.is_empty() {
        return Err(StatusCode::NOT_FOUND);
    }
    let mut user = UserWithRoles {
        id,
        username: rows[0].get("username"),
        roles: Vec::new(),
    };
    for row in rows {
        if let Ok(Some(role)) = row.try_get::<Option<String>, _>("name") {
            user.roles.push(role);
        }
    }
    Ok(Json(user))
}
