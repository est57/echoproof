use anyhow::{Result, anyhow, bail};
use axum::body::Bytes;
use echoproof_core::outbound::OutboundRequest;
use echoproof_core::snapshot::Body;
use http_body_util::{BodyExt, Full, Limited};
use hyper::body::Incoming;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use std::convert::Infallible;
use std::sync::Arc;
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;

use super::OutboundProxy;
use crate::http::{MAX_BODY, content_type, is_hop_header, to_headers};

type Reply = Response<Full<Bytes>>;

pub(super) async fn accept(listener: TcpListener, proxy: Arc<OutboundProxy>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            continue;
        };
        let proxy = proxy.clone();
        tokio::spawn(async move {
            let service = service_fn(move |req| handle(proxy.clone(), req, None));
            let _ = http1::Builder::new()
                .serve_connection(TokioIo::new(stream), service)
                .with_upgrades()
                .await;
        });
    }
}

/// `tls_host` is set inside a CONNECT tunnel, where requests carry only a path.
async fn handle(
    proxy: Arc<OutboundProxy>,
    req: Request<Incoming>,
    tls_host: Option<Arc<str>>,
) -> Result<Reply, Infallible> {
    if req.method() == Method::CONNECT {
        return Ok(connect(proxy, req));
    }
    Ok(match answer(&proxy, req, tls_host.as_deref()).await {
        Ok(reply) => reply,
        Err(error) => text(
            StatusCode::BAD_REQUEST,
            &format!("echoproof proxy: {error:#}"),
        ),
    })
}

fn connect(proxy: Arc<OutboundProxy>, req: Request<Incoming>) -> Reply {
    let Some(authority) = req.uri().authority().cloned() else {
        return text(StatusCode::BAD_REQUEST, "CONNECT needs host:port");
    };
    let host = authority.host().trim_matches(['[', ']']).to_string();
    let label: Arc<str> = match authority.port_u16() {
        Some(443) | None => host.clone().into(),
        Some(port) => format!("{}:{port}", authority.host()).into(),
    };
    tokio::spawn(async move {
        let Ok(upgraded) = hyper::upgrade::on(req).await else {
            return;
        };
        let config = match proxy.tls_config(&host) {
            Ok(config) => config,
            Err(error) => {
                eprintln!("echoproof proxy: cannot issue a certificate for {host}: {error:#}");
                return;
            }
        };
        let Ok(tls) = TlsAcceptor::from(config)
            .accept(TokioIo::new(upgraded))
            .await
        else {
            eprintln!(
                "echoproof proxy: TLS handshake for {host} failed; does the build trust the Echoproof CA?"
            );
            return;
        };
        let service = service_fn(move |req| handle(proxy.clone(), req, Some(label.clone())));
        let _ = http1::Builder::new()
            .serve_connection(TokioIo::new(tls), service)
            .await;
    });
    Response::new(Full::new(Bytes::new()))
}

async fn answer(
    proxy: &OutboundProxy,
    req: Request<Incoming>,
    tls_host: Option<&str>,
) -> Result<Reply> {
    let url = match tls_host {
        Some(host) => {
            let path = req.uri().path_and_query().map_or("/", |p| p.as_str());
            format!("https://{host}{path}")
        }
        None if req.uri().scheme().is_some() => req.uri().to_string(),
        None => bail!("{} is not a proxy request", req.uri()),
    };
    let method = req.method().to_string();
    let mut headers = to_headers(req.headers());
    headers.retain(|name, _| !is_hop_header(name) && name != "proxy-authorization");
    let declared = content_type(req.headers()).map(str::to_string);
    let bytes = Limited::new(req.into_body(), MAX_BODY)
        .collect()
        .await
        .map_err(|error| anyhow!("cannot read request body: {error}"))?
        .to_bytes();
    let recorded = proxy.answer(OutboundRequest {
        method,
        url,
        headers,
        body: Body::from_bytes(&bytes, declared.as_deref()),
    });
    let mut reply = Response::builder().status(StatusCode::from_u16(recorded.status)?);
    for (name, value) in &recorded.headers {
        // Recorded bodies are stored decoded, so their original encoding no longer applies.
        if !is_hop_header(name) && name != "content-encoding" {
            reply = reply.header(name, value);
        }
    }
    Ok(reply.body(Full::new(Bytes::from(recorded.body.to_bytes())))?)
}

fn text(status: StatusCode, message: &str) -> Reply {
    let mut reply = Response::new(Full::new(Bytes::from(message.to_string())));
    *reply.status_mut() = status;
    reply
}
