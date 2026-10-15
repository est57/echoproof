use echoproof_core::outbound::{OutboundCall, OutboundRequest};
use echoproof_core::snapshot::{Body, Response};
use std::collections::{HashMap, VecDeque};

pub const UNRECORDED_STATUS: u16 = 599;

/// The recorded calls of the exchange being replayed, and what the build actually sent.
#[derive(Default)]
pub struct Session {
    active: bool,
    queues: HashMap<String, VecDeque<Response>>,
    captured: Vec<OutboundCall>,
    pub stray: usize,
}

impl Session {
    pub fn begin(&mut self, recorded: &[OutboundCall]) {
        self.active = true;
        self.queues.clear();
        self.captured.clear();
        for call in recorded {
            self.queues
                .entry(call.request.route())
                .or_default()
                .push_back(call.response.clone());
        }
    }

    pub fn finish(&mut self) -> Vec<OutboundCall> {
        self.active = false;
        self.queues.clear();
        std::mem::take(&mut self.captured)
    }

    /// Never reaches the real destination: a call is answered from the recording or refused.
    pub fn answer(&mut self, request: OutboundRequest) -> Response {
        if !self.active {
            self.stray += 1;
            return unrecorded();
        }
        let recorded = self
            .queues
            .get_mut(&request.route())
            .and_then(VecDeque::pop_front);
        let (response, unrecorded) = match recorded {
            Some(response) => (response, false),
            None => (self::unrecorded(), true),
        };
        self.captured.push(OutboundCall {
            request,
            response: response.clone(),
            unrecorded,
        });
        response
    }
}

fn unrecorded() -> Response {
    Response {
        status: UNRECORDED_STATUS,
        headers: Default::default(),
        body: Body::Text("echoproof: unrecorded outbound call".into()),
        latency_ms: 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn request(method: &str, url: &str) -> OutboundRequest {
        OutboundRequest {
            method: method.into(),
            url: url.into(),
            headers: Default::default(),
            body: Body::Json(json!({"amount": 1})),
        }
    }

    fn recorded(url: &str, status: u16) -> OutboundCall {
        OutboundCall {
            request: request("POST", url),
            response: Response {
                status,
                headers: Default::default(),
                body: Body::Empty,
                latency_ms: 0,
            },
            unrecorded: false,
        }
    }

    #[test]
    fn answers_in_recorded_order_and_flags_the_rest() {
        let mut session = Session::default();
        session.begin(&[
            recorded("https://pay.test/v2/charges?key=1", 201),
            recorded("https://pay.test/v2/charges", 402),
        ]);
        assert_eq!(
            session
                .answer(request("POST", "https://pay.test:443/v2/charges"))
                .status,
            201
        );
        assert_eq!(
            session
                .answer(request("POST", "https://pay.test/v2/charges"))
                .status,
            402
        );
        assert_eq!(
            session
                .answer(request("POST", "https://pay.test/v2/charges"))
                .status,
            599
        );
        assert_eq!(
            session
                .answer(request("GET", "https://wa.test/send"))
                .status,
            599
        );
        let captured = session.finish();
        let flags: Vec<bool> = captured.iter().map(|c| c.unrecorded).collect();
        assert_eq!(flags, [false, false, true, true]);
    }

    #[test]
    fn calls_between_exchanges_are_refused_and_counted() {
        let mut session = Session::default();
        assert_eq!(
            session.answer(request("GET", "https://pay.test/x")).status,
            599
        );
        session.begin(&[]);
        session.finish();
        assert_eq!(
            session.answer(request("GET", "https://pay.test/x")).status,
            599
        );
        assert_eq!(session.stray, 2);
        assert!(session.finish().is_empty());
    }
}
