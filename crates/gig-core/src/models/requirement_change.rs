#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequirementChange {
    pub id: i64,
    pub order_id: i64,
    pub description: String,
    pub price_delta: i64,
    pub created_at: i64,
}
