use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: String,
    pub email: String,
    pub password_hash: String,
    pub display_name: String,
    pub college: String,
    pub bio: String,
    pub created_at: String,
    pub verified_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublicProfile {
    pub id: String,
    pub display_name: String,
    pub college: String,
    pub bio: String,
    pub review_count: i64,
    pub avg_overall: Option<f64>,
    pub is_verified: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Review {
    pub id: String,
    pub target_user_id: String,
    pub reviewer_user_id: Option<String>,
    pub anonymous: bool,
    pub comment: String,
    pub cleanliness: i64,
    pub communication: i64,
    pub reliability: i64,
    pub noise: i64,
    pub guests: i64,
    pub overall: i64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ReviewView {
    pub id: String,
    pub author_label: String,
    pub anonymous: bool,
    pub comment: String,
    pub cleanliness: i64,
    pub communication: i64,
    pub reliability: i64,
    pub noise: i64,
    pub guests: i64,
    pub overall: i64,
    pub created_at: String,
    pub target_display_name: String,
    pub target_id: String,
}

#[derive(Debug, Deserialize)]
pub struct SignupForm {
    pub email: String,
    pub password: String,
    pub display_name: String,
    pub college: String,
    pub bio: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SigninForm {
    pub email: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct ReviewForm {
    pub target_user_id: String,
    pub comment: String,
    pub cleanliness: i64,
    pub communication: i64,
    pub reliability: i64,
    pub noise: i64,
    pub guests: i64,
    pub overall: i64,
    pub anonymous: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SearchQuery {
    pub q: Option<String>,
}
