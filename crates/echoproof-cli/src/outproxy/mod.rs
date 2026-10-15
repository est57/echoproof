//! Forward proxy for a service's outbound HTTP(S) calls. While recording it forwards each call and
//! stores it; during replay it answers from the recording and has no upstream code path at all.

mod recording;
mod serve;
pub mod session;

pub use recording::Recording;

use anyhow::{Context, Result, bail};
use echoproof_core::outbound::{OutboundCall, OutboundRequest};
use echoproof_core::snapshot::Response;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use tokio_rustls::rustls::ServerConfig;
use tokio_rustls::rustls::crypto::ring;
use tokio_rustls::rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer};

use crate::ca::Authority;
use session::Session;

pub enum Mode {
    Replay(Mutex<Session>),
    Record(Recording),
}

pub struct OutboundProxy {
    authority: Authority,
    tls: Mutex<HashMap<String, Arc<ServerConfig>>>,
    mode: Mode,
    listener: Mutex<Option<tokio::task::AbortHandle>>,
}

/// Stops the proxy when a check ends, also on errors, so the port is free for the next one.
pub struct Running(pub Option<Arc<OutboundProxy>>);

impl Drop for Running {
    fn drop(&mut self) {
        if let Some(proxy) = &self.0 {
            proxy.stop();
            if proxy.stray() > 0 {
                eprintln!(
                    "echoproof: warning: {} outbound calls reached the proxy while no request was being replayed",
                    proxy.stray()
                );
            }
        }
    }
}

impl OutboundProxy {
    pub async fn start(listen: &str, ca_dir: &Path, mode: Mode) -> Result<Arc<Self>> {
        let addr: SocketAddr = listen.parse().with_context(|| {
            format!(
                "invalid outbound proxy address {listen:?}; use host:port such as 127.0.0.1:8899"
            )
        })?;
        if !addr.ip().is_loopback() {
            bail!("the outbound proxy only listens on a loopback address, not {addr}");
        }
        let proxy = Arc::new(Self {
            authority: Authority::load(ca_dir)?,
            tls: Mutex::default(),
            mode,
            listener: Mutex::default(),
        });
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .with_context(|| format!("cannot listen on {addr}"))?;
        let task = tokio::spawn(serve::accept(listener, proxy.clone()));
        *proxy.listener.lock().unwrap_or_else(|p| p.into_inner()) = Some(task.abort_handle());
        let role = match proxy.mode {
            Mode::Replay(_) => "answering from the recording",
            Mode::Record(_) => "forwarding and recording",
        };
        eprintln!("echoproof: outbound proxy on http://{addr}, {role}");
        Ok(proxy)
    }

    pub fn stop(&self) {
        if let Some(task) = self
            .listener
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take()
        {
            task.abort();
        }
    }

    /// Starts the proxy configured in `[replay] outbound_proxy`, if any.
    pub async fn from_config(address: Option<&str>) -> Result<Running> {
        Ok(Running(match address {
            Some(address) => Some(
                Self::start(
                    address,
                    Path::new(crate::ca::DIR),
                    Mode::Replay(Mutex::default()),
                )
                .await?,
            ),
            None => None,
        }))
    }

    pub fn recording(&self) -> Option<&Recording> {
        match &self.mode {
            Mode::Record(recording) => Some(recording),
            Mode::Replay(_) => None,
        }
    }

    pub fn begin(&self, recorded: &[OutboundCall]) {
        if let Some(mut session) = self.session() {
            session.begin(recorded);
        }
    }

    pub fn finish(&self) -> Vec<OutboundCall> {
        self.session().map(|mut s| s.finish()).unwrap_or_default()
    }

    /// Calls that arrived while no request was being replayed.
    pub fn stray(&self) -> usize {
        self.session().map_or(0, |s| s.stray)
    }

    async fn respond(&self, request: OutboundRequest) -> Response {
        if let Mode::Record(recording) = &self.mode {
            return recording.forward(request).await;
        }
        match self.session() {
            Some(mut session) => session.answer(request),
            None => unreachable!("replay mode always has a session"),
        }
    }

    fn session(&self) -> Option<MutexGuard<'_, Session>> {
        match &self.mode {
            Mode::Replay(session) => Some(
                session
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner()),
            ),
            Mode::Record(_) => None,
        }
    }

    fn tls_config(&self, host: &str) -> Result<Arc<ServerConfig>> {
        let mut cache = self
            .tls
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(config) = cache.get(host) {
            return Ok(config.clone());
        }
        let leaf = self.authority.issue(host)?;
        let mut config = ServerConfig::builder_with_provider(Arc::new(ring::default_provider()))
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(leaf.cert_der)],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(leaf.key_der)),
            )?;
        config.alpn_protocols = vec![b"http/1.1".to_vec()];
        let config = Arc::new(config);
        cache.insert(host.to_string(), config.clone());
        Ok(config)
    }
}
