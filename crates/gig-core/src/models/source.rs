#[derive(Debug, Clone, PartialEq)]
pub struct Source {
    pub id: i64,
    pub name: String,
    pub cut_ratio: f64,
    pub notes: Option<String>,
}
