#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PriceHistoryEntry {
    pub id: i64,
    pub order_id: i64,
    pub old_price: Option<i64>,
    pub new_price: Option<i64>,
    pub reason: Option<String>,
    pub created_at: i64,
}
