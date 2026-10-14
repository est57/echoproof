//! Sends a deterministic mix of POS requests: demo-traffic --target URL [-n 2000]

use serde_json::json;

/// Small deterministic PRNG so every demo run records the same traffic.
struct Lcg(u64);

impl Lcg {
    fn next(&mut self, bound: u64) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (self.0 >> 33) % bound
    }

    fn chance(&mut self, percent: u64) -> bool {
        self.next(100) < percent
    }
}

#[tokio::main]
async fn main() {
    let args: Vec<String> = std::env::args().collect();
    let arg = |flag: &str| {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    };
    let target = arg("--target").unwrap_or_else(|| "http://127.0.0.1:8080".into());
    let count: usize = arg("-n").and_then(|n| n.parse().ok()).unwrap_or(2000);

    let client = reqwest::Client::new();
    let mut rng = Lcg(42);
    let mut failures = 0;
    for _ in 0..count {
        let request = if rng.chance(35) {
            // ids 1..=6; 6 does not exist and returns 404.
            client.get(format!("{target}/products/{}", rng.next(6) + 1))
        } else {
            let items: Vec<_> = if rng.chance(3) {
                Vec::new()
            } else {
                (0..rng.next(3) + 1)
                    .map(|_| json!({ "product_id": rng.next(5) + 1, "qty": rng.next(4) + 1 }))
                    .collect()
            };
            let mut body = json!({ "items": items });
            if rng.chance(30) {
                body["promo_code"] = json!("HEMAT10");
            }
            if rng.chance(30) {
                body["voucher"] = json!("VOUCHER5K");
            }
            client.post(format!("{target}/orders")).json(&body)
        };
        if request.send().await.is_err() {
            failures += 1;
        }
    }
    eprintln!("sent {count} requests to {target} ({failures} transport failures)");
}
