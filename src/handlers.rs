use askama::Template;
use axum::extract::{Form, Path, Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use sqlx::SqlitePool;
use std::sync::Arc;
use tower_cookies::Cookies;
use uuid::Uuid;

use crate::auth::{
    clear_session, current_user_id, hash_password, set_session, verify_password,
};
use crate::db;
use crate::models::{
    PublicProfile, Review, ReviewForm, ReviewView, SearchQuery, SigninForm, SignupForm,
};

mod filters {
    pub fn tier(value: &i64) -> ::askama::Result<&'static str> {
        Ok(match *value {
            5 => "gold",
            4 => "green",
            3 => "blue",
            2 => "yellow",
            1 => "orange",
            _ => "red",
        })
    }

    pub fn tier_avg(value: &Option<f64>) -> ::askama::Result<&'static str> {
        Ok(match *value {
            Some(v) if v >= 5.0 => "gold",
            Some(v) if v >= 4.0 => "green",
            Some(v) if v >= 3.0 => "blue",
            Some(v) if v >= 2.0 => "yellow",
            Some(v) if v >= 1.0 => "orange",
            Some(_) => "red",
            None => "none",
        })
    }
}

#[derive(Clone)]
pub struct AppState {
    pub pool: SqlitePool,
}

pub type SharedState = Arc<AppState>;

#[derive(Template)]
#[template(path = "home.html")]
struct HomeTemplate {
    nav: NavCtx,
    query: String,
    profiles: Vec<PublicProfile>,
    recent: Vec<ReviewView>,
}

#[derive(Template)]
#[template(path = "profile.html")]
struct ProfileTemplate {
    nav: NavCtx,
    profile: PublicProfile,
    reviews: Vec<ReviewView>,
    can_review: bool,
    is_self: bool,
    needs_verify: bool,
}

#[derive(Template)]
#[template(path = "check_email.html")]
struct CheckEmailTemplate {
    nav: NavCtx,
    email: String,
    verify_url: String,
}

#[derive(Template)]
#[template(path = "verified.html")]
struct VerifiedTemplate {
    nav: NavCtx,
    success: bool,
    message: String,
}

#[derive(Template)]
#[template(path = "signup.html")]
struct SignupTemplate {
    nav: NavCtx,
    error: Option<String>,
    email: String,
    display_name: String,
    college: String,
    bio: String,
}

#[derive(Template)]
#[template(path = "signin.html")]
struct SigninTemplate {
    nav: NavCtx,
    error: Option<String>,
    email: String,
}

#[derive(Template)]
#[template(path = "not_found.html")]
struct NotFoundTemplate {
    nav: NavCtx,
}

pub struct NavCtx {
    pub signed_in: bool,
    pub display_name: String,
    pub user_id: String,
    pub verified: bool,
}

async fn build_nav(state: &AppState, cookies: &Cookies) -> NavCtx {
    if let Some(uid) = current_user_id(cookies) {
        if let Ok(Some(user)) = db::find_user_by_id(&state.pool, &uid).await {
            return NavCtx {
                signed_in: true,
                display_name: user.display_name,
                user_id: user.id,
                verified: user.verified_at.is_some(),
            };
        }
    }
    NavCtx {
        signed_in: false,
        display_name: String::new(),
        user_id: String::new(),
        verified: false,
    }
}

fn render<T: Template>(tpl: T) -> Response {
    match tpl.render() {
        Ok(html) => Html(html).into_response(),
        Err(e) => {
            tracing::error!("template error: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "Template error").into_response()
        }
    }
}

pub async fn home(
    State(state): State<SharedState>,
    cookies: Cookies,
    Query(q): Query<SearchQuery>,
) -> Response {
    let nav = build_nav(&state, &cookies).await;
    let query = q.q.unwrap_or_default();
    let profiles = match db::search_profiles(&state.pool, Some(&query)).await {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("search_profiles: {e}");
            Vec::new()
        }
    };
    let recent = db::recent_reviews(&state.pool, 8).await.unwrap_or_default();
    render(HomeTemplate {
        nav,
        query,
        profiles,
        recent,
    })
}

pub async fn profile_page(
    State(state): State<SharedState>,
    cookies: Cookies,
    Path(id): Path<String>,
) -> Response {
    let nav = build_nav(&state, &cookies).await;
    let profile = match db::profile_for_user(&state.pool, &id).await {
        Ok(Some(p)) => p,
        _ => return render(NotFoundTemplate { nav }),
    };
    let reviews = db::reviews_for_target(&state.pool, &id)
        .await
        .unwrap_or_default();

    let is_self = nav.signed_in && nav.user_id == id;
    let can_review = nav.signed_in && !is_self && nav.verified;
    let needs_verify = nav.signed_in && !is_self && !nav.verified;

    render(ProfileTemplate {
        nav,
        profile,
        reviews,
        can_review,
        is_self,
        needs_verify,
    })
}

pub async fn signup_page(State(state): State<SharedState>, cookies: Cookies) -> Response {
    let nav = build_nav(&state, &cookies).await;
    if nav.signed_in {
        return Redirect::to("/").into_response();
    }
    render(SignupTemplate {
        nav,
        error: None,
        email: String::new(),
        display_name: String::new(),
        college: String::new(),
        bio: String::new(),
    })
}

pub async fn signup_submit(
    State(state): State<SharedState>,
    cookies: Cookies,
    Form(form): Form<SignupForm>,
) -> Response {
    let email = form.email.trim().to_lowercase();
    let display_name = form.display_name.trim().to_string();
    let college = form.college.trim().to_string();
    let bio = form.bio.unwrap_or_default().trim().to_string();
    let password = form.password;

    let mut error: Option<String> = None;
    if email.is_empty() || !email.contains('@') {
        error = Some("Please enter a valid email.".into());
    } else if !email.ends_with(".edu") {
        error = Some("Please use your school .edu email so we can verify you.".into());
    } else if password.len() < 8 {
        error = Some("Password must be at least 8 characters.".into());
    } else if display_name.is_empty() {
        error = Some("Display name is required.".into());
    } else if college.is_empty() {
        error = Some("Pick a college so others can find you.".into());
    }

    if error.is_none() {
        match db::find_user_by_email(&state.pool, &email).await {
            Ok(Some(_)) => error = Some("That email is already registered.".into()),
            Ok(None) => {}
            Err(e) => {
                tracing::error!("find_user_by_email: {e}");
                error = Some("Something went wrong. Try again.".into());
            }
        }
    }

    if let Some(err) = error {
        let nav = build_nav(&state, &cookies).await;
        return render(SignupTemplate {
            nav,
            error: Some(err),
            email,
            display_name,
            college,
            bio,
        });
    }

    let hash = match hash_password(&password) {
        Ok(h) => h,
        Err(e) => {
            tracing::error!("hash_password: {e}");
            let nav = build_nav(&state, &cookies).await;
            return render(SignupTemplate {
                nav,
                error: Some("Could not create account. Try again.".into()),
                email,
                display_name,
                college,
                bio,
            });
        }
    };

    let user_id = Uuid::new_v4().to_string();
    if let Err(e) = db::insert_user(
        &state.pool,
        &user_id,
        &email,
        &hash,
        &display_name,
        &college,
        &bio,
    )
    .await
    {
        tracing::error!("insert_user: {e}");
        let nav = build_nav(&state, &cookies).await;
        return render(SignupTemplate {
            nav,
            error: Some("Could not create account. Try again.".into()),
            email,
            display_name,
            college,
            bio,
        });
    }

    set_session(&cookies, &user_id);

    let token = match db::create_verification_token(&state.pool, &user_id).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("create_verification_token: {e}");
            return Redirect::to(&format!("/profile/{}", user_id)).into_response();
        }
    };

    let nav = build_nav(&state, &cookies).await;
    render(CheckEmailTemplate {
        nav,
        email,
        verify_url: format!("/verify/{}", token),
    })
}

pub async fn verify_email(
    State(state): State<SharedState>,
    cookies: Cookies,
    Path(token): Path<String>,
) -> Response {
    let result = db::consume_verification_token(&state.pool, &token).await;
    match result {
        Ok(Some(user_id)) => {
            if current_user_id(&cookies).is_none() {
                set_session(&cookies, &user_id);
            }
            let nav = build_nav(&state, &cookies).await;
            render(VerifiedTemplate {
                nav,
                success: true,
                message: "Your email is verified — you can now leave reviews.".into(),
            })
        }
        Ok(None) => {
            let nav = build_nav(&state, &cookies).await;
            render(VerifiedTemplate {
                nav,
                success: false,
                message: "This verification link is invalid or has expired. Sign in and request a new one.".into(),
            })
        }
        Err(e) => {
            tracing::error!("consume_verification_token: {e}");
            let nav = build_nav(&state, &cookies).await;
            render(VerifiedTemplate {
                nav,
                success: false,
                message: "Something went wrong. Please try again.".into(),
            })
        }
    }
}

pub async fn resend_verification(
    State(state): State<SharedState>,
    cookies: Cookies,
) -> Response {
    let uid = match current_user_id(&cookies) {
        Some(id) => id,
        None => return Redirect::to("/signin").into_response(),
    };

    let user = match db::find_user_by_id(&state.pool, &uid).await {
        Ok(Some(u)) => u,
        _ => return Redirect::to("/").into_response(),
    };

    if user.verified_at.is_some() {
        return Redirect::to("/").into_response();
    }

    let _ = db::invalidate_user_verification_tokens(&state.pool, &uid).await;
    let token = match db::create_verification_token(&state.pool, &uid).await {
        Ok(t) => t,
        Err(e) => {
            tracing::error!("create_verification_token: {e}");
            return Redirect::to("/").into_response();
        }
    };

    let nav = build_nav(&state, &cookies).await;
    render(CheckEmailTemplate {
        nav,
        email: user.email,
        verify_url: format!("/verify/{}", token),
    })
}

pub async fn signin_page(State(state): State<SharedState>, cookies: Cookies) -> Response {
    let nav = build_nav(&state, &cookies).await;
    if nav.signed_in {
        return Redirect::to("/").into_response();
    }
    render(SigninTemplate {
        nav,
        error: None,
        email: String::new(),
    })
}

pub async fn signin_submit(
    State(state): State<SharedState>,
    cookies: Cookies,
    Form(form): Form<SigninForm>,
) -> Response {
    let email = form.email.trim().to_lowercase();
    let user = match db::find_user_by_email(&state.pool, &email).await {
        Ok(Some(u)) => u,
        Ok(None) => {
            let nav = build_nav(&state, &cookies).await;
            return render(SigninTemplate {
                nav,
                error: Some("Email or password is incorrect.".into()),
                email,
            });
        }
        Err(e) => {
            tracing::error!("find_user_by_email: {e}");
            let nav = build_nav(&state, &cookies).await;
            return render(SigninTemplate {
                nav,
                error: Some("Something went wrong. Try again.".into()),
                email,
            });
        }
    };

    if !verify_password(&form.password, &user.password_hash) {
        let nav = build_nav(&state, &cookies).await;
        return render(SigninTemplate {
            nav,
            error: Some("Email or password is incorrect.".into()),
            email,
        });
    }

    set_session(&cookies, &user.id);
    Redirect::to("/").into_response()
}

pub async fn signout(cookies: Cookies) -> Redirect {
    clear_session(&cookies);
    Redirect::to("/")
}

pub async fn submit_review(
    State(state): State<SharedState>,
    cookies: Cookies,
    Form(form): Form<ReviewForm>,
) -> Response {
    let uid = match current_user_id(&cookies) {
        Some(id) => id,
        None => return Redirect::to("/signin").into_response(),
    };

    let user = match db::find_user_by_id(&state.pool, &uid).await {
        Ok(Some(u)) => u,
        _ => return Redirect::to("/signin").into_response(),
    };
    if user.verified_at.is_none() {
        return Redirect::to(&format!("/profile/{}", form.target_user_id)).into_response();
    }

    if uid == form.target_user_id {
        return Redirect::to(&format!("/profile/{}", form.target_user_id)).into_response();
    }

    let comment = form.comment.trim().to_string();
    if comment.is_empty() {
        return Redirect::to(&format!("/profile/{}", form.target_user_id)).into_response();
    }

    let clamp = |v: i64| v.clamp(1, 5);
    let anonymous = form.anonymous.as_deref() == Some("on");

    let review = Review {
        id: Uuid::new_v4().to_string(),
        target_user_id: form.target_user_id.clone(),
        reviewer_user_id: Some(uid),
        anonymous,
        comment,
        cleanliness: clamp(form.cleanliness),
        communication: clamp(form.communication),
        reliability: clamp(form.reliability),
        noise: clamp(form.noise),
        guests: clamp(form.guests),
        overall: clamp(form.overall),
        created_at: String::new(),
    };

    if let Err(e) = db::insert_review(&state.pool, &review).await {
        tracing::error!("insert_review: {e}");
    }

    Redirect::to(&format!("/profile/{}", form.target_user_id)).into_response()
}
