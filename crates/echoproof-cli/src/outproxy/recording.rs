use echoproof_core::outbound::{OutboundCall, OutboundRequest};
use echoproof_core::snapshot::{Body, Response};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use crate::http::{content_type, is_hop_header, read_body, to_headers};

/// Marks calls the recording proxy sends upstream, so one that comes back is refused, not looped.
pub const FORWARDED: &str = "x-echoproof-forwarded";

/// Forwards calls to the real destination while recording and stores each on the one inbound
/// request in flight. With several in flight, a call cannot be attributed and is only counted.
pub struct Recording {
    client: reqwest::Client,
    inflight: Mutex<Inflight>,
}

#[derive(Default)]
struct Inflight {
    next: u64,
    open: Vec<(u64, Vec<OutboundCall>)>,
    unattributed: usize,
}

impl Recording {
    pub fn new() -> anyhow::Result<Self> {
        Ok(Self {
            client: reqwest::Client::builder()
                .no_proxy()
                .redirect(reqwest::redirect::Policy::none())
                .timeout(Duration::from_secs(60))
                .build()?,
            inflight: Mutex::default(),
        })
    }

    pub fn enter(&self) -> u64 {
        let mut inflight = self.lock();
        inflight.next += 1;
        let token = inflight.next;
        inflight.open.push((token, Vec::new()));
        token
    }

    pub fn leave(&self, token: u64) -> Vec<OutboundCall> {
        let mut inflight = self.lock();
        match inflight.open.iter().position(|(t, _)| *t == token) {
            Some(index) => inflight.open.remove(index).1,
            None => Vec::new(),
        }
    }

    pub fn unattributed(&self) -> usize {
        self.lock().unattributed
    }

    pub async fn forward(&self, request: OutboundRequest) -> Response {
        let started = Instant::now();
        let response = match self.send(&request).await {
            Ok(response) => response,
            Err(error) => Response {
                status: 502,
                headers: Default::default(),
                body: Body::Text(format!("echoproof proxy: {error:#}")),
                latency_ms: 0,
            },
        };
        let response = Response {
            latency_ms: started.elapsed().as_millis() as u64,
            ..response
        };
        let mut inflight = self.lock();
        match inflight.open.as_mut_slice() {
            [(_, calls)] => calls.push(OutboundCall {
                request,
                response: response.clone(),
                unrecorded: false,
            }),
            _ => inflight.unattributed += 1,
        }
        response
    }

    async fn send(&self, request: &OutboundRequest) -> anyhow::Result<Response> {
        let method = reqwest::Method::from_bytes(request.method.as_bytes())?;
        let mut upstream = self
            .client
            .request(method, &request.url)
            .header(FORWARDED, "1");
        for (name, value) in &request.headers {
            if !is_hop_header(name) {
                upstream = upstream.header(name, value);
            }
        }
        let response = upstream.body(request.body.to_bytes()).send().await?;
        let status = response.status().as_u16();
        let headers = response.headers().clone();
        let bytes = read_body(response).await?;
        Ok(Response {
            status,
            body: Body::from_bytes(&bytes, content_type(&headers)),
            headers: to_headers(&headers),
            latency_ms: 0,
        })
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, Inflight> {
        self.inflight
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}
