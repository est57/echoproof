use anyhow::{Context, Result};
use axum::Router;
use axum::body::Body as AxumBody;
use axum::extract::{Request, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use parity_core::sanitize::Sanitizer;
use parity_core::snapshot::{self, Body, Exchange, SnapshotWriter};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::http::{MAX_BODY, content_type, forwardable, read_body, to_headers};
use crate::replay::parse_target;

struct Recorder {
    client: reqwest::Client,
    upstream: String,
    sanitizer: Sanitizer,
    writer: Mutex<SnapshotWriter>,
    recorded: AtomicU64,
}

pub async fn run(upstream: &str, listen: &str, out: &Path, sanitizer: Sanitizer) -> Result<()> {
    parse_target(upstream)?;
    let recorder = Arc::new(Recorder {
        client: reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()?,
        upstream: upstream.trim_end_matches('/').to_string(),
        sanitizer,
        writer: Mutex::new(SnapshotWriter::create(out)?),
        recorded: AtomicU64::new(0),
    });
    let app = Router::new().fallback(proxy).with_state(recorder.clone());
    let listener = tokio::net::TcpListener::bind(listen)
        .await
        .with_context(|| format!("cannot listen on {listen}"))?;
    eprintln!(
        "parity record: http://{listen} -> {upstream}, writing {} (Ctrl-C to stop)",
        out.display()
    );
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;
    let count = recorder.recorded.load(Ordering::Relaxed);
    eprintln!(
        "parity record: {count} exchanges written to {}",
        out.display()
    );
    Ok(())
}

async fn proxy(State(recorder): State<Arc<Recorder>>, request: Request) -> Response {
    match forward(&recorder, request).await {
        Ok(response) => response,
        Err(error) => {
            eprintln!("parity record: {error:#}");
            (StatusCode::BAD_GATEWAY, "parity proxy error").into_response()
        }
    }
}

async fn forward(recorder: &Recorder, request: Request) -> Result<Response> {
    let (parts, body) = request.into_parts();
    let request_bytes = axum::body::to_bytes(body, MAX_BODY).await?;
    let path = parts
        .uri
        .path_and_query()
        .map_or("/", |p| p.as_str())
        .to_string();
    let mut upstream_request = recorder
        .client
        .request(parts.method.clone(), format!("{}{path}", recorder.upstream));
    for (name, value) in parts.headers.iter().filter(|(name, _)| forwardable(name)) {
        upstream_request = upstream_request.header(name, value);
    }
    let started = Instant::now();
    let upstream_response = upstream_request.body(request_bytes.clone()).send().await?;
    let status = upstream_response.status();
    let response_headers = upstream_response.headers().clone();
    let response_bytes = read_body(upstream_response).await?;

    let mut exchange = Exchange {
        id: uuid::Uuid::new_v4().simple().to_string()[..12].to_string(),
        recorded_at_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64),
        request: snapshot::Request {
            method: parts.method.to_string(),
            path,
            headers: to_headers(&parts.headers),
            body: Body::from_bytes(&request_bytes, content_type(&parts.headers)),
        },
        response: snapshot::Response {
            status: status.as_u16(),
            headers: to_headers(&response_headers),
            body: Body::from_bytes(&response_bytes, content_type(&response_headers)),
            latency_ms: started.elapsed().as_millis() as u64,
        },
        skipped: None,
    };
    recorder.sanitizer.apply(&mut exchange);
    let written = recorder
        .writer
        .lock()
        .map_err(|_| anyhow::anyhow!("snapshot writer poisoned"))?
        .append(&exchange);
    match written {
        Ok(()) => {
            recorder.recorded.fetch_add(1, Ordering::Relaxed);
        }
        Err(error) => eprintln!("parity record: failed to write exchange: {error:#}"),
    }

    let mut response = Response::builder().status(status);
    for (name, value) in response_headers
        .iter()
        .filter(|(name, _)| forwardable(name))
    {
        response = response.header(name, value);
    }
    Ok(response.body(AxumBody::from(response_bytes))?)
}

async fn shutdown_signal() {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let Ok(mut term) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        else {
            let _ = ctrl_c.await;
            return;
        };
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
}
