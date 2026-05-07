use anyhow::Result;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use uuid::Uuid;

use crate::models::{PublicProfile, Review, ReviewView, User};

pub async fn init_pool(url: &str) -> Result<SqlitePool> {
    let opts = SqliteConnectOptions::from_str(url)?
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(8)
        .connect_with(opts)
        .await?;
    init_schema(&pool).await?;
    Ok(pool)
}

async fn init_schema(pool: &SqlitePool) -> Result<()> {
    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS users (
            id TEXT PRIMARY KEY,
            email TEXT NOT NULL UNIQUE,
            password_hash TEXT NOT NULL,
            display_name TEXT NOT NULL,
            college TEXT NOT NULL,
            bio TEXT NOT NULL DEFAULT '',
            created_at TEXT NOT NULL DEFAULT (datetime('now')),
            verified_at TEXT
        );
        "#,
    )
    .execute(pool)
    .await?;

    if let Err(e) = sqlx::query("ALTER TABLE users ADD COLUMN verified_at TEXT")
        .execute(pool)
        .await
    {
        if !e.to_string().contains("duplicate column") {
            return Err(e.into());
        }
    }

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS reviews (
            id TEXT PRIMARY KEY,
            target_user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            reviewer_user_id TEXT REFERENCES users(id) ON DELETE SET NULL,
            anonymous INTEGER NOT NULL DEFAULT 1,
            comment TEXT NOT NULL,
            cleanliness INTEGER NOT NULL,
            communication INTEGER NOT NULL,
            reliability INTEGER NOT NULL,
            noise INTEGER NOT NULL,
            guests INTEGER NOT NULL,
            overall INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query("CREATE INDEX IF NOT EXISTS idx_reviews_target ON reviews(target_user_id);")
        .execute(pool)
        .await?;

    sqlx::query(
        r#"
        CREATE TABLE IF NOT EXISTS email_verifications (
            id TEXT PRIMARY KEY,
            user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
            token TEXT NOT NULL UNIQUE,
            expires_at TEXT NOT NULL,
            consumed_at TEXT,
            created_at TEXT NOT NULL DEFAULT (datetime('now'))
        );
        "#,
    )
    .execute(pool)
    .await?;

    sqlx::query(
        "CREATE INDEX IF NOT EXISTS idx_email_verifications_user ON email_verifications(user_id);",
    )
    .execute(pool)
    .await?;

    Ok(())
}

pub async fn find_user_by_email(pool: &SqlitePool, email: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE email = ?")
        .bind(email)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

pub async fn find_user_by_id(pool: &SqlitePool, id: &str) -> Result<Option<User>> {
    let user = sqlx::query_as::<_, User>("SELECT * FROM users WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    Ok(user)
}

pub async fn insert_user(
    pool: &SqlitePool,
    id: &str,
    email: &str,
    password_hash: &str,
    display_name: &str,
    college: &str,
    bio: &str,
) -> Result<()> {
    sqlx::query(
        "INSERT INTO users (id, email, password_hash, display_name, college, bio) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(id)
    .bind(email)
    .bind(password_hash)
    .bind(display_name)
    .bind(college)
    .bind(bio)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn search_profiles(pool: &SqlitePool, query: Option<&str>) -> Result<Vec<PublicProfile>> {
    let q = query.map(|s| s.trim()).filter(|s| !s.is_empty());

    let rows = if let Some(qstr) = q {
        let like = format!("%{}%", qstr);
        sqlx::query_as::<_, (String, String, String, String, i64, Option<f64>, Option<String>)>(
            r#"
            SELECT u.id, u.display_name, u.college, u.bio,
                   COUNT(r.id) AS review_count,
                   AVG(r.overall) AS avg_overall,
                   u.verified_at
            FROM users u
            LEFT JOIN reviews r ON r.target_user_id = u.id
            WHERE u.display_name LIKE ? OR u.college LIKE ?
            GROUP BY u.id
            ORDER BY u.display_name COLLATE NOCASE
            "#,
        )
        .bind(&like)
        .bind(&like)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, (String, String, String, String, i64, Option<f64>, Option<String>)>(
            r#"
            SELECT u.id, u.display_name, u.college, u.bio,
                   COUNT(r.id) AS review_count,
                   AVG(r.overall) AS avg_overall,
                   u.verified_at
            FROM users u
            LEFT JOIN reviews r ON r.target_user_id = u.id
            GROUP BY u.id
            ORDER BY u.display_name COLLATE NOCASE
            "#,
        )
        .fetch_all(pool)
        .await?
    };

    Ok(rows
        .into_iter()
        .map(|(id, display_name, college, bio, review_count, avg_overall, verified_at)| {
            PublicProfile {
                id,
                display_name,
                college,
                bio,
                review_count,
                avg_overall,
                is_verified: verified_at.is_some(),
            }
        })
        .collect())
}

pub async fn profile_for_user(pool: &SqlitePool, user_id: &str) -> Result<Option<PublicProfile>> {
    let row = sqlx::query_as::<_, (String, String, String, String, i64, Option<f64>, Option<String>)>(
        r#"
        SELECT u.id, u.display_name, u.college, u.bio,
               COUNT(r.id) AS review_count,
               AVG(r.overall) AS avg_overall,
               u.verified_at
        FROM users u
        LEFT JOIN reviews r ON r.target_user_id = u.id
        WHERE u.id = ?
        GROUP BY u.id
        "#,
    )
    .bind(user_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|(id, display_name, college, bio, review_count, avg_overall, verified_at)| {
        PublicProfile {
            id,
            display_name,
            college,
            bio,
            review_count,
            avg_overall,
            is_verified: verified_at.is_some(),
        }
    }))
}

pub async fn reviews_for_target(pool: &SqlitePool, target_user_id: &str) -> Result<Vec<ReviewView>> {
    let rows = sqlx::query_as::<_, (
        String, String, Option<String>, bool, String,
        i64, i64, i64, i64, i64, i64, String,
        String, String, Option<String>,
    )>(
        r#"
        SELECT r.id, r.target_user_id, r.reviewer_user_id, r.anonymous, r.comment,
               r.cleanliness, r.communication, r.reliability, r.noise, r.guests, r.overall,
               r.created_at,
               t.display_name AS target_display_name, t.id AS t_id,
               rv.display_name AS reviewer_display_name
        FROM reviews r
        JOIN users t ON t.id = r.target_user_id
        LEFT JOIN users rv ON rv.id = r.reviewer_user_id
        WHERE r.target_user_id = ?
        ORDER BY r.created_at DESC
        "#,
    )
    .bind(target_user_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, _target_user_id, _reviewer_user_id, anonymous, comment,
              cleanliness, communication, reliability, noise, guests, overall,
              created_at, target_display_name, target_id, reviewer_display_name)| {
            let author_label = if anonymous {
                "Anonymous".to_string()
            } else {
                reviewer_display_name.unwrap_or_else(|| "Anonymous".to_string())
            };
            ReviewView {
                id,
                author_label,
                anonymous,
                comment,
                cleanliness,
                communication,
                reliability,
                noise,
                guests,
                overall,
                created_at,
                target_display_name,
                target_id,
            }
        })
        .collect())
}

pub async fn recent_reviews(pool: &SqlitePool, limit: i64) -> Result<Vec<ReviewView>> {
    let rows = sqlx::query_as::<_, (
        String, String, Option<String>, bool, String,
        i64, i64, i64, i64, i64, i64, String,
        String, String, Option<String>,
    )>(
        r#"
        SELECT r.id, r.target_user_id, r.reviewer_user_id, r.anonymous, r.comment,
               r.cleanliness, r.communication, r.reliability, r.noise, r.guests, r.overall,
               r.created_at,
               t.display_name AS target_display_name, t.id AS t_id,
               rv.display_name AS reviewer_display_name
        FROM reviews r
        JOIN users t ON t.id = r.target_user_id
        LEFT JOIN users rv ON rv.id = r.reviewer_user_id
        ORDER BY r.created_at DESC
        LIMIT ?
        "#,
    )
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|(id, _t, _rv, anonymous, comment,
              cleanliness, communication, reliability, noise, guests, overall,
              created_at, target_display_name, target_id, reviewer_display_name)| {
            let author_label = if anonymous {
                "Anonymous".to_string()
            } else {
                reviewer_display_name.unwrap_or_else(|| "Anonymous".to_string())
            };
            ReviewView {
                id,
                author_label,
                anonymous,
                comment,
                cleanliness,
                communication,
                reliability,
                noise,
                guests,
                overall,
                created_at,
                target_display_name,
                target_id,
            }
        })
        .collect())
}

pub async fn insert_review(pool: &SqlitePool, review: &Review) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO reviews (id, target_user_id, reviewer_user_id, anonymous,
                             comment, cleanliness, communication, reliability,
                             noise, guests, overall)
        VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
        "#,
    )
    .bind(&review.id)
    .bind(&review.target_user_id)
    .bind(&review.reviewer_user_id)
    .bind(review.anonymous)
    .bind(&review.comment)
    .bind(review.cleanliness)
    .bind(review.communication)
    .bind(review.reliability)
    .bind(review.noise)
    .bind(review.guests)
    .bind(review.overall)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn create_verification_token(pool: &SqlitePool, user_id: &str) -> Result<String> {
    let id = Uuid::new_v4().to_string();
    let token = Uuid::new_v4().simple().to_string();
    sqlx::query(
        "INSERT INTO email_verifications (id, user_id, token, expires_at) VALUES (?, ?, ?, datetime('now', '+24 hours'))",
    )
    .bind(&id)
    .bind(user_id)
    .bind(&token)
    .execute(pool)
    .await?;
    Ok(token)
}

pub async fn consume_verification_token(pool: &SqlitePool, token: &str) -> Result<Option<String>> {
    let row: Option<(String,)> = sqlx::query_as(
        "SELECT user_id FROM email_verifications WHERE token = ? AND consumed_at IS NULL AND expires_at > datetime('now')",
    )
    .bind(token)
    .fetch_optional(pool)
    .await?;

    let Some((user_id,)) = row else { return Ok(None) };

    sqlx::query("UPDATE email_verifications SET consumed_at = datetime('now') WHERE token = ?")
        .bind(token)
        .execute(pool)
        .await?;
    sqlx::query("UPDATE users SET verified_at = datetime('now') WHERE id = ? AND verified_at IS NULL")
        .bind(&user_id)
        .execute(pool)
        .await?;
    Ok(Some(user_id))
}

pub async fn invalidate_user_verification_tokens(pool: &SqlitePool, user_id: &str) -> Result<()> {
    sqlx::query(
        "UPDATE email_verifications SET consumed_at = datetime('now') WHERE user_id = ? AND consumed_at IS NULL",
    )
    .bind(user_id)
    .execute(pool)
    .await?;
    Ok(())
}

pub async fn user_count(pool: &SqlitePool) -> Result<i64> {
    let (count,): (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users")
        .fetch_one(pool)
        .await?;
    Ok(count)
}

pub async fn seed_if_empty(pool: &SqlitePool) -> Result<()> {
    if user_count(pool).await? > 0 {
        return Ok(());
    }

    use crate::auth::hash_password;

    let demo_password = hash_password("password123")?;

    let demos = [
        (
            "Jordan P.",
            "jordan@demo.app",
            "Boston University",
            "Quiet during weeknights, keeps common spaces tidy, loves clear chore schedules.",
        ),
        (
            "Samira L.",
            "samira@demo.app",
            "UT Austin",
            "Friendly and social, communicates plans early, always pays bills on time.",
        ),
        (
            "Chris M.",
            "chris@demo.app",
            "NYU",
            "Night-owl creative. Respects boundaries, hosts the occasional weekend hangout.",
        ),
    ];

    let mut user_ids = Vec::new();
    for (name, email, college, bio) in demos.iter() {
        let id = Uuid::new_v4().to_string();
        insert_user(pool, &id, email, &demo_password, name, college, bio).await?;
        user_ids.push(id);
    }

    let demo_reviews = [
        (0, "Aly", false, "Super respectful and easy to coordinate chores with. Cleaned the kitchen without me ever asking.", 5, 4, 5, 5, 4, 5),
        (0, "anon", true, "Solid roommate overall. A bit shy at first but warmed up.", 4, 4, 5, 5, 5, 4),
        (1, "Derek", false, "Very social and kind. Just talk about quiet hours up front.", 4, 5, 4, 3, 3, 4),
        (2, "anon", true, "Cool to live with if you keep similar hours. Bathroom can pile up.", 3, 4, 4, 3, 3, 4),
    ];

    for (idx, _name, anon, comment, cl, co, re, no, gu, ov) in demo_reviews.iter() {
        let target_id = &user_ids[*idx];
        let r = Review {
            id: Uuid::new_v4().to_string(),
            target_user_id: target_id.clone(),
            reviewer_user_id: None,
            anonymous: *anon,
            comment: comment.to_string(),
            cleanliness: *cl,
            communication: *co,
            reliability: *re,
            noise: *no,
            guests: *gu,
            overall: *ov,
            created_at: String::new(),
        };
        insert_review(pool, &r).await?;
    }

    sqlx::query("UPDATE users SET verified_at = datetime('now') WHERE verified_at IS NULL")
        .execute(pool)
        .await?;

    Ok(())
}
