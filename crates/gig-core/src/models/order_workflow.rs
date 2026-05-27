use super::ProjectType;

#[derive(Debug, Clone, PartialEq)]
pub struct OrderWorkflow {
    pub order_id: i64,
    pub project_type: Option<ProjectType>,
    pub gig_dir: Option<String>,
    pub index_path: Option<String>,
    pub job_path: Option<String>,
    pub quote_path: Option<String>,
    pub plan_md_path: Option<String>,
    pub plan_html_path: Option<String>,
    pub plan_ready_at: Option<String>,
    pub plan_approved_at: Option<String>,
    pub plan_rejected_at: Option<String>,
    pub plan_rejection_reason: Option<String>,
    pub acceptance_path: Option<String>,
    pub acceptance_completed_at: Option<String>,
    pub latest_delivery_dir: Option<String>,
    pub latest_client_package_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}
