use std::collections::HashSet;

pub const CATALOG: &[(u32, &str, i64)] = &[
    (1, "Kopi Susu", 18_000),
    (2, "Roti Bakar", 15_000),
    (3, "Nasi Goreng", 32_000),
    (4, "Teh Manis", 8_000),
    (5, "Mie Ayam", 25_000),
];

/// Behavior changes that stand in for pull requests; see eval/judge/CASES.md.
const KNOWN: &[&str] = &[
    "currency",
    "voucher_first",
    "promo15",
    "voucher10k",
    "promo_min_50k",
    "bulk5_qty4",
    "round_total_1000",
    "price_roti_16k",
    "rename_total",
    "drop_line_total",
    "empty_items_400",
    "error_capitalized",
    "upper_names",
];

#[derive(Debug, Default)]
pub struct Mutations(HashSet<&'static str>);

impl Mutations {
    /// `DEMO_MUTATIONS=a,b`; `DEMO_VERSION=2` is the demo's agent PR.
    pub fn from_env() -> Result<Self, String> {
        let mut names: Vec<String> = std::env::var("DEMO_MUTATIONS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        if std::env::var("DEMO_VERSION").as_deref() == Ok("2") {
            names.extend(["currency".into(), "voucher_first".into()]);
        }
        let mut on = HashSet::new();
        for name in &names {
            let known = KNOWN
                .iter()
                .find(|k| *k == name)
                .ok_or(format!("unknown mutation: {name}"))?;
            on.insert(*known);
        }
        Ok(Self(on))
    }

    pub fn on(&self, name: &str) -> bool {
        self.0.contains(name)
    }

    pub fn product(&self, id: u32) -> Option<(u32, String, i64)> {
        let (id, name, mut price) = CATALOG.iter().copied().find(|(pid, _, _)| *pid == id)?;
        if self.on("price_roti_16k") && id == 2 {
            price = 16_000;
        }
        let name = if self.on("upper_names") {
            name.to_uppercase()
        } else {
            name.to_string()
        };
        Some((id, name, price))
    }

    pub fn discount(
        &self,
        subtotal: i64,
        bulk_lines: i64,
        promo: Option<&str>,
        voucher: Option<&str>,
    ) -> i64 {
        let promo_applies =
            promo == Some("HEMAT10") && !(self.on("promo_min_50k") && subtotal < 50_000);
        let promo_percent = match (promo_applies, self.on("promo15")) {
            (false, _) => 0,
            (true, false) => 10,
            (true, true) => 15,
        };
        let voucher_amount = match (voucher == Some("VOUCHER5K"), self.on("voucher10k")) {
            (false, _) => 0,
            (true, false) => 5_000,
            (true, true) => 10_000,
        };
        let mut discount = if self.on("voucher_first") {
            voucher_amount + (subtotal - voucher_amount).max(0) * promo_percent / 100
        } else {
            subtotal * promo_percent / 100 + voucher_amount
        };
        if self.on("bulk5_qty4") {
            discount += bulk_lines * 5 / 100;
        }
        discount.min(subtotal)
    }
}
