use crate::auth::token_is_valid;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::{Json, Router};
use gig_core::config::{Config, Paths};
use gig_core::models::{Order, OrderWorkflow, QuoteDraft};
use gig_core::repo::orders::ListFilter;
use gig_core::repo::{order_workflow, orders};
use gig_core::services::dashboard::{self, Dashboard, DashboardItem, DashboardQuoteItem};
use serde::Serialize;
use std::collections::BTreeMap;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct AppState {
    pub paths: Paths,
    pub config: Config,
    pub token: String,
}

#[derive(Debug, Serialize)]
struct ApiErrorBody {
    status: &'static str,
    error: ApiErrorDetail,
}

#[derive(Debug, Serialize)]
struct ApiErrorDetail {
    code: &'static str,
    message: String,
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    status: &'static str,
    app: &'static str,
}

#[derive(Debug, Serialize)]
struct DashboardResponse {
    summary: DashboardSummaryView,
    quotes: Vec<QuoteDashboardView>,
    focus: Vec<DashboardOrderView>,
}

#[derive(Debug, Serialize)]
struct DashboardSummaryView {
    this_month_income: i64,
    orders_count: usize,
    pending_count: usize,
}

#[derive(Debug, Serialize)]
struct QuoteDashboardView {
    draft: QuoteDraftView,
    next_action: &'static str,
}

#[derive(Debug, Serialize)]
struct DashboardOrderView {
    order: OrderView,
    workflow: Option<WorkflowView>,
    next_action: &'static str,
    alert: Option<String>,
}

#[derive(Debug, Serialize)]
struct OrdersResponse {
    orders: Vec<OrderWithWorkflowView>,
}

#[derive(Debug, Serialize)]
struct OrderResponse {
    order: OrderWithWorkflowView,
}

#[derive(Debug, Serialize)]
struct OrderWithWorkflowView {
    order: OrderView,
    workflow: Option<WorkflowView>,
    next_action: &'static str,
}

#[derive(Debug, Serialize)]
struct OrderView {
    id: i64,
    slug: Option<String>,
    title: String,
    status: String,
    project_type: Option<String>,
    quoted_price: Option<i64>,
    final_price: Option<i64>,
    my_cut_amount: Option<i64>,
    currency: String,
    dev_path: Option<String>,
    archive_path: Option<String>,
    created_at: i64,
    accepted_at: Option<i64>,
    delivered_at: Option<i64>,
    paid_at: Option<i64>,
    archived_at: Option<i64>,
}

#[derive(Debug, Serialize)]
struct WorkflowView {
    order_id: i64,
    project_type: Option<String>,
    gig_dir: Option<String>,
    index_path: Option<String>,
    job_path: Option<String>,
    quote_path: Option<String>,
    plan_md_path: Option<String>,
    plan_html_path: Option<String>,
    plan_ready_at: Option<String>,
    plan_approved_at: Option<String>,
    plan_rejected_at: Option<String>,
    work_started_at: Option<String>,
    plan_rejection_reason: Option<String>,
    acceptance_path: Option<String>,
    acceptance_completed_at: Option<String>,
    latest_delivery_dir: Option<String>,
    latest_client_package_path: Option<String>,
}

#[derive(Debug, Serialize)]
struct QuoteDraftView {
    id: i64,
    slug: String,
    title: String,
    client_label: Option<String>,
    source_org: Option<String>,
    project_type: String,
    status: String,
    summary: String,
    quote_min: Option<i64>,
    quote_recommended: Option<i64>,
    quote_max: Option<i64>,
    currency: String,
    created_at: String,
    updated_at: String,
    quoted_at: Option<String>,
    sent_at: Option<String>,
}

#[derive(Debug, Serialize)]
struct RedactedConfigResponse {
    general: RedactedGeneral,
    pack: RedactedPack,
    delivery: RedactedDelivery,
    paths: RedactedPaths,
}

#[derive(Debug, Serialize)]
struct RedactedGeneral {
    dev_root: String,
    archive_root: String,
    default_cut_ratio: f64,
    default_currency: String,
}

#[derive(Debug, Serialize)]
struct RedactedPack {
    default_format: String,
    extra_ignore: Vec<String>,
}

#[derive(Debug, Serialize)]
struct RedactedDelivery {
    default_uploader: String,
    short_link: RedactedShortLink,
    s3: BTreeMap<String, RedactedS3>,
}

#[derive(Debug, Serialize)]
struct RedactedShortLink {
    enabled: bool,
    endpoint: String,
    token_set: bool,
}

#[derive(Debug, Serialize)]
struct RedactedS3 {
    bucket: String,
    region: String,
    endpoint: String,
    download_endpoint: Option<String>,
    access_key_set: bool,
    secret_key_set: bool,
    link_ttl_seconds: u32,
    path_style: bool,
}

#[derive(Debug, Serialize)]
struct RedactedPaths {
    data_dir: String,
    config_file: String,
    db_file: String,
    state_dir: String,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/dashboard", get(dashboard_handler))
        .route("/api/orders", get(orders_handler))
        .route("/api/orders/:id", get(order_handler))
        .route("/api/config", get(config_handler))
        .with_state(state)
}

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        app: "gig-gui",
    })
}

async fn dashboard_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DashboardResponse>, Response> {
    require_auth(&headers, &state)?;
    let paths = state.paths.clone();
    let now = OffsetDateTime::now_utc().unix_timestamp();
    let dashboard = run_blocking(move || {
        let conn = gig_core::db::open(&paths.db_file).map_err(|err| err.to_string())?;
        dashboard::build_dashboard(&conn, now).map_err(|err| err.to_string())
    })
    .await?;
    Ok(Json(dashboard_response(dashboard)))
}

async fn orders_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrdersResponse>, Response> {
    require_auth(&headers, &state)?;
    let paths = state.paths.clone();
    let orders = run_blocking(move || {
        let conn = gig_core::db::open(&paths.db_file).map_err(|err| err.to_string())?;
        let rows =
            orders::list(&conn, &ListFilter { status: None }).map_err(|err| err.to_string())?;
        rows.into_iter()
            .map(|order| order_with_workflow_view(&conn, order))
            .collect::<Result<Vec<_>, String>>()
    })
    .await?;
    Ok(Json(OrdersResponse { orders }))
}

async fn order_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<OrderResponse>, Response> {
    require_auth(&headers, &state)?;
    let paths = state.paths.clone();
    let order = run_blocking(move || {
        let conn = gig_core::db::open(&paths.db_file).map_err(|err| err.to_string())?;
        let order = orders::find_by_id(&conn, id).map_err(|err| err.to_string())?;
        order_with_workflow_view(&conn, order)
    })
    .await?;
    Ok(Json(OrderResponse { order }))
}

async fn config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RedactedConfigResponse>, Response> {
    require_auth(&headers, &state)?;
    Ok(Json(redacted_config(&state.paths, &state.config)))
}

fn require_auth(headers: &HeaderMap, state: &AppState) -> Result<(), Response> {
    if token_is_valid(headers, &state.token) {
        Ok(())
    } else {
        Err(api_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        ))
    }
}

async fn run_blocking<T, F>(work: F) -> Result<T, Response>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|err| {
            api_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "join_error",
                err.to_string(),
            )
        })?
        .map_err(|message| api_error(StatusCode::INTERNAL_SERVER_ERROR, "core_error", message))
}

fn api_error(status: StatusCode, code: &'static str, message: impl Into<String>) -> Response {
    (
        status,
        Json(ApiErrorBody {
            status: "error",
            error: ApiErrorDetail {
                code,
                message: message.into(),
            },
        }),
    )
        .into_response()
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
) -> Result<OrderWithWorkflowView, String> {
    let workflow =
        order_workflow::find_by_order_id(conn, order.id).map_err(|err| err.to_string())?;
    let next_action = dashboard::order_next_action(conn, &order, workflow.as_ref())
        .map_err(|err| err.to_string())?;
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
                },
            )
        })
        .collect();

    RedactedConfigResponse {
        general: RedactedGeneral {
            dev_root: paths_string(&config.general.dev_root),
            archive_root: paths_string(&config.general.archive_root),
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
            data_dir: paths_string(&paths.data_dir),
            config_file: paths_string(&paths.config_file),
            db_file: paths_string(&paths.db_file),
            state_dir: paths_string(&paths.state_dir),
        },
    }
}

fn paths_string(path: &std::path::Path) -> String {
    path.to_string_lossy().into_owned()
}

const INDEX_HTML: &str = r#"<!doctype html>
<html lang="zh-CN">
<head>
  <meta charset="utf-8" />
  <meta name="viewport" content="width=device-width, initial-scale=1" />
  <title>gig gui</title>
  <style>
    :root { color-scheme: dark; font-family: ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif; }
    body { margin: 0; background: #0d0d0b; color: #f1efe7; }
    main { max-width: 1120px; margin: 0 auto; padding: 40px 24px 72px; }
    header { display: flex; justify-content: space-between; gap: 24px; align-items: flex-end; border-bottom: 1px solid #302e29; padding-bottom: 24px; }
    h1 { font-size: clamp(40px, 7vw, 88px); letter-spacing: -0.08em; line-height: 0.9; margin: 0; }
    .subtitle { color: #a7a095; margin: 12px 0 0; max-width: 520px; }
    .pill { border: 1px solid #3e392f; border-radius: 999px; padding: 8px 12px; color: #d3c7aa; font-size: 12px; white-space: nowrap; }
    .grid { display: grid; grid-template-columns: repeat(3, minmax(0, 1fr)); gap: 12px; margin-top: 24px; }
    .card { background: #171612; border: 1px solid #302e29; border-radius: 18px; padding: 18px; }
    .metric { font-size: 34px; font-weight: 750; letter-spacing: -0.04em; }
    .label { color: #a7a095; font-size: 13px; margin-top: 4px; }
    .section { margin-top: 28px; }
    h2 { font-size: 18px; letter-spacing: 0.08em; text-transform: uppercase; color: #d3c7aa; }
    table { width: 100%; border-collapse: collapse; overflow: hidden; border-radius: 16px; }
    th, td { border-bottom: 1px solid #2a2824; padding: 12px 10px; text-align: left; vertical-align: top; }
    th { color: #a7a095; font-size: 12px; font-weight: 600; }
    td { font-size: 14px; }
    .muted { color: #a7a095; }
    .alert { color: #f2b66d; }
    .error { border-color: #7f2d2d; color: #fecaca; }
    @media (max-width: 760px) { header { display: block; } .grid { grid-template-columns: 1fr; } table { display: block; overflow-x: auto; } }
  </style>
</head>
<body>
  <main>
    <header>
      <div>
        <h1>gig gui</h1>
        <p class="subtitle">Read-only local cockpit. Existing CLI remains isolated; this page reads through gig-core APIs.</p>
      </div>
      <div class="pill" id="auth-state">checking token</div>
    </header>

    <section class="grid" id="metrics"></section>

    <section class="section">
      <h2>Focus</h2>
      <div class="card" id="focus">Loading…</div>
    </section>

    <section class="section">
      <h2>Quotes</h2>
      <div class="card" id="quotes">Loading…</div>
    </section>
  </main>

  <script>
    const params = new URLSearchParams(location.search);
    const urlToken = params.get('token');
    if (urlToken) sessionStorage.setItem('gigGuiToken', urlToken);
    const token = sessionStorage.getItem('gigGuiToken');
    const authState = document.getElementById('auth-state');

    async function api(path) {
      const res = await fetch(path, { headers: { Authorization: `Bearer ${token || ''}` } });
      if (!res.ok) throw new Error(`${res.status} ${await res.text()}`);
      return res.json();
    }

    function money(minor) {
      return `¥${(minor / 100).toFixed(2)}`;
    }

    function rows(items, kind) {
      if (!items.length) return '<p class="muted">None</p>';
      return `<table><thead><tr><th>ID</th><th>Title</th><th>Status</th><th>Next</th><th>Alert</th></tr></thead><tbody>${items.map(item => {
        const entity = kind === 'quote' ? item.draft : item.order;
        return `<tr><td>${entity.id}</td><td>${escapeHtml(entity.title)}</td><td>${escapeHtml(entity.status)}</td><td>${escapeHtml(item.next_action)}</td><td class="alert">${escapeHtml(item.alert || '')}</td></tr>`;
      }).join('')}</tbody></table>`;
    }

    function escapeHtml(value) {
      return String(value).replace(/[&<>"]/g, ch => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[ch]));
    }

    async function boot() {
      if (!token) throw new Error('missing token in URL');
      const data = await api('/api/dashboard');
      authState.textContent = 'localhost token ok';
      document.getElementById('metrics').innerHTML = [
        ['This month', money(data.summary.this_month_income)],
        ['Active orders', data.summary.orders_count],
        ['Pending payment', data.summary.pending_count],
      ].map(([label, value]) => `<div class="card"><div class="metric">${value}</div><div class="label">${label}</div></div>`).join('');
      document.getElementById('focus').innerHTML = rows(data.focus, 'order');
      document.getElementById('quotes').innerHTML = rows(data.quotes, 'quote');
    }

    boot().catch(err => {
      authState.textContent = 'auth failed';
      authState.classList.add('error');
      document.getElementById('focus').innerHTML = `<p class="alert">${escapeHtml(err.message)}</p>`;
      document.getElementById('quotes').innerHTML = '<p class="muted">Unavailable</p>';
    });
  </script>
</body>
</html>"#;

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{to_bytes, Body};
    use axum::http::header::AUTHORIZATION;
    use axum::http::Request;
    use gig_core::config::{
        DeliveryConfig, General, PackConfig, S3UploaderConfig, ShortLinkConfig,
    };
    use serde_json::Value;
    use tower::ServiceExt;

    const TEST_TOKEN: &str = "test-token";
    const SHORT_LINK_SECRET: &str = "short-secret";
    const S3_ACCESS_SECRET: &str = "access-secret";
    const S3_SECRET_SECRET: &str = "secret-secret";

    fn test_config() -> Config {
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
            },
        );
        config
    }

    fn test_router() -> (Router, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        let app = router(AppState {
            paths,
            config: test_config(),
            token: TEST_TOKEN.into(),
        });
        (app, tmp)
    }

    async fn get(app: Router, uri: &str, token: Option<&str>) -> Response {
        let mut request = Request::builder().uri(uri);
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        app.oneshot(request.body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    async fn json_body(response: Response) -> Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn health_is_public() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/health", None).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["status"], "ok");
        assert_eq!(body["app"], "gig-gui");
    }

    #[tokio::test]
    async fn dashboard_rejects_missing_token() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/dashboard", None).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let body = json_body(response).await;
        assert_eq!(body["status"], "error");
        assert_eq!(body["error"]["code"], "unauthorized");
    }

    #[tokio::test]
    async fn dashboard_accepts_bearer_token() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/dashboard", Some(TEST_TOKEN)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["summary"]["this_month_income"], 0);
        assert_eq!(body["summary"]["orders_count"], 0);
        assert_eq!(body["summary"]["pending_count"], 0);
        assert!(body["quotes"].as_array().unwrap().is_empty());
        assert!(body["focus"].as_array().unwrap().is_empty());
    }

    #[tokio::test]
    async fn config_api_redacts_secrets() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/config", Some(TEST_TOKEN)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(bytes.to_vec()).unwrap();
        assert!(!text.contains(SHORT_LINK_SECRET));
        assert!(!text.contains(S3_ACCESS_SECRET));
        assert!(!text.contains(S3_SECRET_SECRET));

        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["delivery"]["short_link"]["token_set"], true);
        assert_eq!(body["delivery"]["s3"]["main"]["access_key_set"], true);
        assert_eq!(body["delivery"]["s3"]["main"]["secret_key_set"], true);
    }

    #[test]
    fn redacted_config_hides_secrets() {
        let paths = Paths::under_root(tempfile::tempdir().unwrap().path());

        let redacted = redacted_config(&paths, &test_config());

        assert!(redacted.delivery.short_link.token_set);
        let text = serde_json::to_string(&redacted).unwrap();
        assert!(!text.contains(SHORT_LINK_SECRET));
        assert!(!text.contains(S3_ACCESS_SECRET));
        assert!(!text.contains(S3_SECRET_SECRET));
    }
}
