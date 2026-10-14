//! Small POS backend with stock (state) for the Echoproof demo and eval.

mod mutations;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, MutexGuard};

use mutations::{CATALOG, Mutations};

const INITIAL_STOCK: i64 = 5_000;

#[derive(Deserialize)]
struct OrderRequest {
    items: Vec<OrderItem>,
    promo_code: Option<String>,
    voucher: Option<String>,
}

#[derive(Deserialize)]
struct OrderItem {
    product_id: u32,
    qty: u32,
}

struct Shop {
    m: Mutations,
    stock: Mutex<HashMap<u32, i64>>,
}

type Reply = (StatusCode, Json<Value>);

impl Shop {
    fn initial_stock() -> HashMap<u32, i64> {
        CATALOG
            .iter()
            .map(|(id, _, _)| (*id, INITIAL_STOCK))
            .collect()
    }

    fn stock(&self) -> MutexGuard<'_, HashMap<u32, i64>> {
        self.stock
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn error(status: StatusCode, message: impl Into<String>) -> Reply {
    (status, Json(json!({ "error": message.into() })))
}

async fn reset(State(shop): State<Arc<Shop>>) -> StatusCode {
    *shop.stock() = Shop::initial_stock();
    StatusCode::NO_CONTENT
}

async fn get_product(State(shop): State<Arc<Shop>>, Path(id): Path<u32>) -> Reply {
    let Some((id, name, price)) = shop.m.product(id) else {
        let message = if shop.m.on("error_capitalized") {
            "Product not found"
        } else {
            "product not found"
        };
        return error(StatusCode::NOT_FOUND, message);
    };
    let stock = shop.stock().get(&id).copied().unwrap_or(0);
    let mut body = json!({ "id": id, "name": name, "price": price, "stock": stock });
    if shop.m.on("currency") {
        body["currency"] = json!("IDR");
    }
    (StatusCode::OK, Json(body))
}

async fn create_order(State(shop): State<Arc<Shop>>, Json(order): Json<OrderRequest>) -> Reply {
    let m = &shop.m;
    if order.items.is_empty() {
        let status = if m.on("empty_items_400") {
            StatusCode::BAD_REQUEST
        } else {
            StatusCode::UNPROCESSABLE_ENTITY
        };
        return error(status, "items must not be empty");
    }
    let (mut lines, mut subtotal, mut bulk_lines) = (Vec::new(), 0i64, 0i64);
    for item in &order.items {
        let Some((id, name, price)) = m.product(item.product_id) else {
            return error(
                StatusCode::UNPROCESSABLE_ENTITY,
                format!("unknown product {}", item.product_id),
            );
        };
        let line_total = price * i64::from(item.qty);
        subtotal += line_total;
        if item.qty >= 4 {
            bulk_lines += line_total;
        }
        let mut line = json!({ "product_id": id, "name": name, "qty": item.qty });
        if !m.on("drop_line_total") {
            line["line_total"] = json!(line_total);
        }
        lines.push(line);
    }
    {
        let mut stock = shop.stock();
        for item in &order.items {
            *stock.entry(item.product_id).or_default() -= i64::from(item.qty);
        }
    }
    let discount = m.discount(
        subtotal,
        bulk_lines,
        order.promo_code.as_deref(),
        order.voucher.as_deref(),
    );
    let mut total = subtotal - discount;
    if m.on("round_total_1000") {
        total = (total + 500) / 1_000 * 1_000;
    }
    let mut body = json!({
        "order_id": uuid::Uuid::new_v4().to_string(),
        "created_at": chrono::Utc::now().to_rfc3339(),
        "items": lines,
        "subtotal": subtotal,
        "discount": discount,
    });
    body[if m.on("rename_total") {
        "grand_total"
    } else {
        "total"
    }] = json!(total);
    (StatusCode::CREATED, Json(body))
}

#[tokio::main]
async fn main() {
    let mutations = match Mutations::from_env() {
        Ok(mutations) => mutations,
        Err(message) => {
            eprintln!("demo-shop: {message}");
            std::process::exit(2);
        }
    };
    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".into());
    eprintln!("demo-shop listening on http://127.0.0.1:{port} with {mutations:?}");
    let shop = Arc::new(Shop {
        m: mutations,
        stock: Mutex::new(Shop::initial_stock()),
    });
    let app = Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/products/{id}", get(get_product))
        .route("/orders", post(create_order))
        .route("/__reset", post(reset))
        .with_state(shop);
    let listener = match tokio::net::TcpListener::bind(format!("127.0.0.1:{port}")).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("demo-shop: cannot listen on port {port}: {error}");
            std::process::exit(2);
        }
    };
    if let Err(error) = axum::serve(listener, app).await {
        eprintln!("demo-shop: {error}");
    }
}
