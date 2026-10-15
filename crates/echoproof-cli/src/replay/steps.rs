use echoproof_core::idmap::IdMap;
use echoproof_core::snapshot::{Body, Exchange};

use super::ReplayOptions;

/// Opens the proxy session for one exchange and stores what the build sent through it.
pub(super) async fn through_proxy(
    options: &ReplayOptions,
    exchange: &Exchange,
    send: impl Future<Output = Exchange>,
) -> Exchange {
    let Some(proxy) = &options.outbound else {
        return send.await;
    };
    proxy.begin(exchange.outbound.as_deref().unwrap_or_default());
    let mut candidate = send.await;
    let mut calls = candidate.outbound.take().unwrap_or_default();
    calls.extend(proxy.finish());
    candidate.outbound = Some(calls);
    candidate
}

/// The recorded request with IDs created earlier in this replay put in place of recorded ones.
pub(super) fn rewrite(exchange: &Exchange, ids: &IdMap) -> Exchange {
    let mut out = exchange.clone();
    if ids.is_empty() {
        return out;
    }
    out.request.path = ids.rewrite_path(&exchange.request.path);
    if let Body::Json(body) = &mut out.request.body {
        ids.rewrite_body(&exchange.request.path, body);
    }
    out
}

pub(super) fn learn(ids: &mut IdMap, recorded: &Exchange, replayed: &Exchange) {
    if replayed.skipped.is_some() {
        return;
    }
    if let (Body::Json(before), Body::Json(after)) =
        (&recorded.response.body, &replayed.response.body)
    {
        ids.learn(&recorded.request.path, before, after);
    }
}
