#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Client {
    pub id: i64,
    pub display_name: String,
    pub wechat_contact: Option<String>,
    pub source_org: Option<String>,
    pub notes: Option<String>,
    pub first_seen_at: i64,
}
