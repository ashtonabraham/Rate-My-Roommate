mod auth;
mod db;
mod handlers;
mod models;

use axum::routing::{get, post};
use axum::Router;
use std::net::SocketAddr;
use std::sync::Arc;
use tower_cookies::CookieManagerLayer;
use tower_http::services::ServeDir;
use tower_http::trace::TraceLayer;

use handlers::AppState;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rate_my_roomate=info,tower_http=info".into()),
        )
        .init();

    let db_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "sqlite://data/app.db".to_string());
    let pool = db::init_pool(&db_url).await?;
    db::seed_if_empty(&pool).await?;

    let state = Arc::new(AppState { pool });

    let app = Router::new()
        .route("/", get(handlers::home))
        .route("/profile/:id", get(handlers::profile_page))
        .route("/signup", get(handlers::signup_page).post(handlers::signup_submit))
        .route("/signin", get(handlers::signin_page).post(handlers::signin_submit))
        .route("/signout", post(handlers::signout))
        .route("/reviews", post(handlers::submit_review))
        .nest_service("/static", ServeDir::new("static"))
        .layer(CookieManagerLayer::new())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let addr: SocketAddr = std::env::var("BIND_ADDR")
        .unwrap_or_else(|_| "127.0.0.1:3000".to_string())
        .parse()?;
    tracing::info!("listening on http://{addr}");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}
