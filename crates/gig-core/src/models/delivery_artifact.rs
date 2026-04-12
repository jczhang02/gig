//! DeliveryArtifact model — maps to the `delivery_artifacts` table.

#[derive(Debug, Clone, PartialEq)]
pub struct DeliveryArtifact {
    pub id: i64,
    pub order_id: i64,
    pub local_path: Option<String>,
    pub uploader_name: Option<String>,
    pub remote_url: Option<String>,
    pub expires_at: Option<i64>,
    pub uploaded_at: i64,
}
