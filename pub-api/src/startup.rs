use axum::{
    extract::Request,
    routing::{get, post},
    serve::Serve,
    Router,
};
use sqlx::PgPool;
use std::{sync::Arc, time::Duration};
use tokio::net::TcpListener;
use tower_http::{classify::ServerErrorsFailureClass, trace::TraceLayer};
use tracing::{error, info_span, Span};

use crate::{
    routes::tasks::{check_status, create_task},
    state::AppState,
};

pub fn get_server(listener: TcpListener, db_pool: PgPool) -> Serve<Router, Router> {
    let tracing_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request<_>| {
            let request_id = uuid::Uuid::new_v4();
            info_span!(
                "http_request",
                method = ?request.method(),
                matched_path = %request.uri().path(),
                %request_id
            )
        })
        .on_failure(
            |err: ServerErrorsFailureClass, _latency: Duration, _span: &Span| {
                error!(error = %err);
            },
        );

    let share_state = Arc::new(AppState { pool: db_pool });
    let router = Router::new()
        .route("/health-check", get(health_check))
        .route("/task/create", post(create_task))
        .route("/task/status", post(check_status))
        .layer(tracing_layer)
        .with_state(share_state);

    axum::serve(listener, router)
}

async fn health_check() -> &'static str {
    "OK"
}