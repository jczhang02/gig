use super::context::ActionContext;
use crate::config::{Config, Paths};
use crate::context::{canonical, resolve_context_for};
use crate::models::{Order, OrderStatus, OrderWorkflow, QuoteDraft};
use crate::repo::orders::ListFilter;
use crate::repo::{order_workflow, orders};
use crate::services::dashboard::{self, Dashboard, DashboardItem, DashboardQuoteItem};
use crate::{Error, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ReadActionInput {
    #[default]
    Empty,
    OrdersList {
        status: Option<OrderStatus>,
    },
    OrderDetail {
        #[serde(alias = "id")]
        id_or_slug: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "type", content = "data", rename_all = "snake_case")]
pub enum ReadActionOutput {
    Dashboard(Box<DashboardResponse>),
    Orders(Box<OrdersResponse>),
    Order(Box<OrderResponse>),
    Config(Box<RedactedConfigResponse>),
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DashboardResponse {
    pub summary: DashboardSummaryView,
    pub quotes: Vec<QuoteDashboardView>,
    pub focus: Vec<DashboardOrderView>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DashboardSummaryView {
    pub this_month_income: i64,
    pub orders_count: usize,
    pub pending_count: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuoteDashboardView {
    pub draft: QuoteDraftView,
    pub next_action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DashboardOrderView {
    pub order: OrderView,
    pub workflow: Option<WorkflowView>,
    pub next_action: &'static str,
    pub alert: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrdersResponse {
    pub orders: Vec<OrderWithWorkflowView>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderResponse {
    pub order: OrderWithWorkflowView,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderWithWorkflowView {
    pub order: OrderView,
    pub workflow: Option<WorkflowView>,
    pub next_action: &'static str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OrderView {
    pub id: i64,
    pub slug: Option<String>,
    pub title: String,
    pub status: String,
    pub project_type: Option<String>,
    pub quoted_price: Option<i64>,
    pub final_price: Option<i64>,
    pub my_cut_amount: Option<i64>,
    pub currency: String,
    pub dev_path: Option<String>,
    pub archive_path: Option<String>,
    pub created_at: i64,
    pub accepted_at: Option<i64>,
    pub delivered_at: Option<i64>,
    pub paid_at: Option<i64>,
    pub archived_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct WorkflowView {
    pub order_id: i64,
    pub project_type: Option<String>,
    pub gig_dir: Option<String>,
    pub index_path: Option<String>,
    pub job_path: Option<String>,
    pub quote_path: Option<String>,
    pub plan_md_path: Option<String>,
    pub plan_html_path: Option<String>,
    pub plan_ready_at: Option<String>,
    pub plan_approved_at: Option<String>,
    pub plan_rejected_at: Option<String>,
    pub work_started_at: Option<String>,
    pub plan_rejection_reason: Option<String>,
    pub acceptance_path: Option<String>,
    pub acceptance_completed_at: Option<String>,
    pub latest_delivery_dir: Option<String>,
    pub latest_client_package_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct QuoteDraftView {
    pub id: i64,
    pub slug: String,
    pub title: String,
    pub client_label: Option<String>,
    pub source_org: Option<String>,
    pub project_type: String,
    pub status: String,
    pub summary: String,
    pub quote_min: Option<i64>,
    pub quote_recommended: Option<i64>,
    pub quote_max: Option<i64>,
    pub currency: String,
    pub created_at: String,
    pub updated_at: String,
    pub quoted_at: Option<String>,
    pub sent_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RedactedConfigResponse {
    pub general: RedactedGeneral,
    pub pack: RedactedPack,
    pub delivery: RedactedDelivery,
    pub paths: RedactedPaths,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RedactedGeneral {
    pub dev_root: String,
    pub archive_root: String,
    pub default_cut_ratio: f64,
    pub default_currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedPack {
    pub default_format: String,
    pub extra_ignore: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedDelivery {
    pub default_uploader: String,
    pub short_link: RedactedShortLink,
    pub s3: BTreeMap<String, RedactedS3>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedShortLink {
    pub enabled: bool,
    pub endpoint: String,
    pub token_set: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedS3 {
    pub bucket: String,
    pub region: String,
    pub endpoint: String,
    pub download_endpoint: Option<String>,
    pub access_key_set: bool,
    pub secret_key_set: bool,
    pub link_ttl_seconds: u32,
    pub path_style: bool,
    pub allow_insecure_http: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RedactedPaths {
    pub data_dir: String,
    pub config_file: String,
    pub db_file: String,
    pub state_dir: String,
}

pub fn is_read_action(action_id: &str) -> bool {
    matches!(
        action_id,
        "dashboard.get" | "orders.list" | "orders.get_detail" | "config.redacted.get"
    )
}

pub fn execute_read_action(
    ctx: &ActionContext<'_>,
    action_id: &str,
    input: ReadActionInput,
) -> Result<ReadActionOutput> {
    match action_id {
        "dashboard.get" => require_empty(action_id, input).map(|()| {
            get_dashboard(ctx).map(|output| ReadActionOutput::Dashboard(Box::new(output)))
        })?,
        "orders.list" => {
            let status = match input {
                ReadActionInput::Empty => None,
                ReadActionInput::OrdersList { status } => status,
                other => return Err(invalid_input(action_id, other)),
            };
            list_orders(ctx, status).map(|output| ReadActionOutput::Orders(Box::new(output)))
        }
        "orders.get_detail" => {
            let id_or_slug = match input {
                ReadActionInput::Empty => None,
                ReadActionInput::OrderDetail { id_or_slug } => id_or_slug,
                other => return Err(invalid_input(action_id, other)),
            };
            get_order_detail(ctx, id_or_slug.as_deref())
                .map(|output| ReadActionOutput::Order(Box::new(output)))
        }
        "config.redacted.get" => require_empty(action_id, input)
            .and_then(|()| get_redacted_config(ctx))
            .map(|output| ReadActionOutput::Config(Box::new(output))),
        other => Err(Error::Invalid(format!(
            "unknown or non-read action for read executor: {other}"
        ))),
    }
}

pub fn get_dashboard(ctx: &ActionContext<'_>) -> Result<DashboardResponse> {
    dashboard::build_dashboard(ctx.conn, ctx.now).map(dashboard_response)
}

pub fn list_orders(ctx: &ActionContext<'_>, status: Option<OrderStatus>) -> Result<OrdersResponse> {
    let rows = orders::list(ctx.conn, &ListFilter { status })?;
    let orders = rows
        .into_iter()
        .map(|order| order_with_workflow_view(ctx.conn, order))
        .collect::<Result<Vec<_>>>()?;
    Ok(OrdersResponse { orders })
}

pub fn get_order_detail(
    ctx: &ActionContext<'_>,
    id_or_slug: Option<&str>,
) -> Result<OrderResponse> {
    let order = resolve_order(ctx, id_or_slug)?;
    Ok(OrderResponse {
        order: order_with_workflow_view(ctx.conn, order)?,
    })
}

pub fn get_redacted_config(ctx: &ActionContext<'_>) -> Result<RedactedConfigResponse> {
    get_redacted_config_for_paths(ctx.paths)
}

pub fn get_redacted_config_for_paths(paths: &Paths) -> Result<RedactedConfigResponse> {
    let config = Config::load_or_default(&paths.config_file)?;
    Ok(redacted_config(paths, &config))
}

fn resolve_order(ctx: &ActionContext<'_>, id_or_slug: Option<&str>) -> Result<Order> {
    if let Some(id_or_slug) = id_or_slug.filter(|value| !value.trim().is_empty()) {
        return orders::find_by_id_or_slug(ctx.conn, id_or_slug);
    }

    let cwd = canonical(ctx.cwd)?;
    let order_id = resolve_context_for(ctx.conn, &cwd)?.ok_or_else(|| {
        Error::Invalid(
            "not inside a gig project directory; pass an Order id_or_slug or cd into one".into(),
        )
    })?;
    orders::find_by_id(ctx.conn, order_id)
}

fn require_empty(action_id: &str, input: ReadActionInput) -> Result<()> {
    match input {
        ReadActionInput::Empty => Ok(()),
        other => Err(invalid_input(action_id, other)),
    }
}

fn invalid_input(action_id: &str, input: ReadActionInput) -> Error {
    Error::Invalid(format!("invalid input for {action_id}: {input:?}"))
}

fn dashboard_response(dashboard: Dashboard) -> DashboardResponse {
    DashboardResponse {
        summary: DashboardSummaryView {
            this_month_income: dashboard.summary.this_month_income,
            orders_count: dashboard.summary.orders_count,
            pending_count: dashboard.summary.pending_count,
        },
        quotes: dashboard
            .quote_items
            .into_iter()
            .map(quote_dashboard_view)
            .collect(),
        focus: dashboard
            .focus_items
            .into_iter()
            .map(dashboard_order_view)
            .collect(),
    }
}

fn quote_dashboard_view(item: DashboardQuoteItem) -> QuoteDashboardView {
    QuoteDashboardView {
        draft: quote_draft_view(item.draft),
        next_action: item.next_action,
    }
}

fn dashboard_order_view(item: DashboardItem) -> DashboardOrderView {
    DashboardOrderView {
        order: order_view(item.order),
        workflow: item.workflow.map(workflow_view),
        next_action: item.next_action,
        alert: item.alert,
    }
}

fn order_with_workflow_view(
    conn: &rusqlite::Connection,
    order: Order,
) -> Result<OrderWithWorkflowView> {
    let workflow = order_workflow::find_by_order_id(conn, order.id)?;
    let next_action = dashboard::order_next_action(conn, &order, workflow.as_ref())?;
    Ok(OrderWithWorkflowView {
        order: order_view(order),
        workflow: workflow.map(workflow_view),
        next_action,
    })
}

fn order_view(order: Order) -> OrderView {
    let my_cut_amount = order.my_cut_amount();
    OrderView {
        id: order.id,
        slug: order.slug,
        title: order.title,
        status: order.status.as_str().to_string(),
        project_type: order.project_type.map(|value| value.as_str().to_string()),
        quoted_price: order.quoted_price,
        final_price: order.final_price,
        my_cut_amount,
        currency: order.currency,
        dev_path: order.dev_path,
        archive_path: order.archive_path,
        created_at: order.created_at,
        accepted_at: order.accepted_at,
        delivered_at: order.delivered_at,
        paid_at: order.paid_at,
        archived_at: order.archived_at,
    }
}

fn workflow_view(workflow: OrderWorkflow) -> WorkflowView {
    WorkflowView {
        order_id: workflow.order_id,
        project_type: workflow
            .project_type
            .map(|value| value.as_str().to_string()),
        gig_dir: workflow.gig_dir,
        index_path: workflow.index_path,
        job_path: workflow.job_path,
        quote_path: workflow.quote_path,
        plan_md_path: workflow.plan_md_path,
        plan_html_path: workflow.plan_html_path,
        plan_ready_at: workflow.plan_ready_at,
        plan_approved_at: workflow.plan_approved_at,
        plan_rejected_at: workflow.plan_rejected_at,
        work_started_at: workflow.work_started_at,
        plan_rejection_reason: workflow.plan_rejection_reason,
        acceptance_path: workflow.acceptance_path,
        acceptance_completed_at: workflow.acceptance_completed_at,
        latest_delivery_dir: workflow.latest_delivery_dir,
        latest_client_package_path: workflow.latest_client_package_path,
    }
}

fn quote_draft_view(draft: QuoteDraft) -> QuoteDraftView {
    QuoteDraftView {
        id: draft.id,
        slug: draft.slug,
        title: draft.title,
        client_label: draft.client_label,
        source_org: draft.source_org,
        project_type: draft.project_type.as_str().to_string(),
        status: draft.status.as_str().to_string(),
        summary: draft.summary,
        quote_min: draft.quote_min,
        quote_recommended: draft.quote_recommended,
        quote_max: draft.quote_max,
        currency: draft.currency,
        created_at: draft.created_at,
        updated_at: draft.updated_at,
        quoted_at: draft.quoted_at,
        sent_at: draft.sent_at,
    }
}

fn redacted_config(paths: &Paths, config: &Config) -> RedactedConfigResponse {
    let s3 = config
        .delivery
        .s3
        .iter()
        .map(|(name, cfg)| {
            (
                name.clone(),
                RedactedS3 {
                    bucket: cfg.bucket.clone(),
                    region: cfg.region.clone(),
                    endpoint: cfg.endpoint.clone(),
                    download_endpoint: cfg.download_endpoint.clone(),
                    access_key_set: !cfg.access_key.trim().is_empty(),
                    secret_key_set: !cfg.secret_key.trim().is_empty(),
                    link_ttl_seconds: cfg.link_ttl_seconds,
                    path_style: cfg.path_style,
                    allow_insecure_http: cfg.allow_insecure_http,
                },
            )
        })
        .collect();

    RedactedConfigResponse {
        general: RedactedGeneral {
            dev_root: path_string(&config.general.dev_root),
            archive_root: path_string(&config.general.archive_root),
            default_cut_ratio: config.general.default_cut_ratio,
            default_currency: config.general.default_currency.clone(),
        },
        pack: RedactedPack {
            default_format: config.pack.default_format.clone(),
            extra_ignore: config.pack.extra_ignore.clone(),
        },
        delivery: RedactedDelivery {
            default_uploader: config.delivery.default_uploader.clone(),
            short_link: RedactedShortLink {
                enabled: config.delivery.short_link.enabled,
                endpoint: config.delivery.short_link.endpoint.clone(),
                token_set: !config.delivery.short_link.token.trim().is_empty(),
            },
            s3,
        },
        paths: RedactedPaths {
            data_dir: path_string(&paths.data_dir),
            config_file: path_string(&paths.config_file),
            db_file: path_string(&paths.db_file),
            state_dir: path_string(&paths.state_dir),
        },
    }
}

fn path_string(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{
        Config, DeliveryConfig, General, PackConfig, Paths, S3UploaderConfig, ShortLinkConfig,
    };
    use crate::db::open_in_memory;
    use crate::models::{OrderStatus, ProjectType, QuoteDraftStatus};
    use crate::repo::orders::NewOrder;
    use crate::repo::quote_drafts::{self, NewQuoteDraft};
    use std::path::Path;

    const NOW: i64 = 1_780_128_000;
    const SHORT_LINK_SECRET: &str = "short-secret";
    const S3_ACCESS_SECRET: &str = "access-secret";
    const S3_SECRET_SECRET: &str = "secret-secret";

    fn insert_order(conn: &rusqlite::Connection) -> i64 {
        orders::insert(
            conn,
            &NewOrder {
                slug: Some("phase-c"),
                external_id: None,
                title: "Phase C implementation",
                client_id: None,
                source_org: None,
                source_id: None,
                project_type: Some(ProjectType::FrontendWeb),
                status: OrderStatus::Lead,
                quoted_price: Some(30_000),
                final_price: Some(40_000),
                my_cut_ratio: 0.6,
                currency: "CNY",
                notes: None,
                created_at: NOW - 3 * 86_400,
                accepted_at: None,
            },
        )
        .unwrap()
        .id
    }

    fn test_paths(root: &Path) -> Paths {
        Paths::under_root(root)
    }

    fn secret_config() -> Config {
        let mut config = Config {
            general: General::default(),
            pack: PackConfig::default(),
            delivery: DeliveryConfig::default(),
        };
        config.delivery.default_uploader = "s3:main".into();
        config.delivery.short_link = ShortLinkConfig {
            enabled: true,
            endpoint: "https://go.example.test/api/links".into(),
            token: SHORT_LINK_SECRET.into(),
        };
        config.delivery.s3.insert(
            "main".into(),
            S3UploaderConfig {
                bucket: "gig-delivery".into(),
                region: "cn-hongkong".into(),
                endpoint: "https://s3.oss-cn-hongkong.aliyuncs.com".into(),
                download_endpoint: Some("https://oss-accelerate.aliyuncs.com".into()),
                access_key: S3_ACCESS_SECRET.into(),
                secret_key: S3_SECRET_SECRET.into(),
                link_ttl_seconds: 604_800,
                path_style: false,
                allow_insecure_http: false,
            },
        );
        config
    }

    #[test]
    fn dashboard_get_returns_summary_focus_and_quote_views() {
        let conn = open_in_memory().unwrap();
        insert_order(&conn);
        let tmp = tempfile::tempdir().unwrap();
        let paths = test_paths(tmp.path());
        let config = Config::default();
        let ctx = ActionContext::new(&conn, &paths, &config, tmp.path(), NOW);
        quote_drafts::insert(
            &conn,
            &NewQuoteDraft {
                slug: "quote-1",
                title: "Quote one",
                client_label: Some("Client"),
                source_org: None,
                project_type: ProjectType::Crawler,
                status: QuoteDraftStatus::QuoteDraft,
                summary: "Need crawler",
                quote_min: None,
                quote_recommended: None,
                quote_max: None,
                currency: "CNY",
                xdg_path: "/tmp/quote-1",
                created_at: "2026-05-30T00:00:00Z",
                updated_at: "2026-05-30T00:00:00Z",
            },
        )
        .unwrap();

        let output = execute_read_action(&ctx, "dashboard.get", ReadActionInput::Empty).unwrap();

        let ReadActionOutput::Dashboard(dashboard) = output else {
            panic!("expected dashboard output");
        };
        assert_eq!(dashboard.summary.orders_count, 1);
        assert_eq!(dashboard.focus[0].order.slug.as_deref(), Some("phase-c"));
        assert_eq!(dashboard.focus[0].next_action, "qualify_lead");
        assert_eq!(dashboard.quotes[0].draft.slug, "quote-1");
        assert_eq!(dashboard.quotes[0].next_action, "price_quote");
    }

    #[test]
    fn orders_list_returns_order_and_next_action() {
        let conn = open_in_memory().unwrap();
        let order_id = insert_order(&conn);
        let tmp = tempfile::tempdir().unwrap();
        let paths = test_paths(tmp.path());
        let config = Config::default();
        let ctx = ActionContext::new(&conn, &paths, &config, tmp.path(), NOW);

        let output = execute_read_action(
            &ctx,
            "orders.list",
            ReadActionInput::OrdersList { status: None },
        )
        .unwrap();

        let ReadActionOutput::Orders(list) = output else {
            panic!("expected orders output");
        };
        assert_eq!(list.orders.len(), 1);
        assert_eq!(list.orders[0].order.id, order_id);
        assert_eq!(list.orders[0].order.my_cut_amount, Some(24_000));
        assert_eq!(list.orders[0].next_action, "qualify_lead");
    }

    #[test]
    fn orders_get_detail_accepts_id_slug_and_context() {
        let conn = open_in_memory().unwrap();
        let order_id = insert_order(&conn);
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("phase-c");
        std::fs::create_dir_all(&project_dir).unwrap();
        conn.execute(
            "UPDATE orders SET dev_path = ?1 WHERE id = ?2",
            (project_dir.to_string_lossy().as_ref(), order_id),
        )
        .unwrap();
        let paths = test_paths(tmp.path());
        let config = Config::default();
        let ctx = ActionContext::new(&conn, &paths, &config, &project_dir, NOW);

        let output = execute_read_action(
            &ctx,
            "orders.get_detail",
            ReadActionInput::OrderDetail {
                id_or_slug: Some(order_id.to_string()),
            },
        )
        .unwrap();
        let ReadActionOutput::Order(detail) = output else {
            panic!("expected order output");
        };
        assert_eq!(detail.order.order.slug.as_deref(), Some("phase-c"));

        let legacy_id_input: ReadActionInput =
            serde_json::from_str(r#"{"type":"order_detail","id":"phase-c"}"#).unwrap();
        let output = execute_read_action(&ctx, "orders.get_detail", legacy_id_input).unwrap();
        let ReadActionOutput::Order(detail) = output else {
            panic!("expected order output");
        };
        assert_eq!(detail.order.order.id, order_id);

        let output = execute_read_action(
            &ctx,
            "orders.get_detail",
            ReadActionInput::OrderDetail {
                id_or_slug: Some("phase-c".into()),
            },
        )
        .unwrap();
        let ReadActionOutput::Order(detail) = output else {
            panic!("expected order output");
        };
        assert_eq!(detail.order.order.id, order_id);

        let output =
            execute_read_action(&ctx, "orders.get_detail", ReadActionInput::Empty).unwrap();
        let ReadActionOutput::Order(detail) = output else {
            panic!("expected order output");
        };
        assert_eq!(detail.order.order.id, order_id);
    }

    #[test]
    fn config_redaction_hides_secret_values() {
        let conn = open_in_memory().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let paths = test_paths(tmp.path());
        let config = secret_config();
        config.save(&paths.config_file).unwrap();
        let default_config = Config::default();
        let ctx = ActionContext::new(&conn, &paths, &default_config, tmp.path(), NOW);

        let output =
            execute_read_action(&ctx, "config.redacted.get", ReadActionInput::Empty).unwrap();
        let ReadActionOutput::Config(redacted) = output else {
            panic!("expected config output");
        };

        assert!(redacted.delivery.short_link.token_set);
        assert!(redacted.delivery.s3["main"].access_key_set);
        assert!(redacted.delivery.s3["main"].secret_key_set);
        let text = serde_json::to_string(&redacted).unwrap();
        assert!(!text.contains(SHORT_LINK_SECRET));
        assert!(!text.contains(S3_ACCESS_SECRET));
        assert!(!text.contains(S3_SECRET_SECRET));
    }
}
