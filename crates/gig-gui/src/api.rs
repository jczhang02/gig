use crate::auth::token_is_valid;
use axum::body::Bytes;
use axum::extract::{Path as AxumPath, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use gig_core::actions::{
    self, execute_read_action, get_redacted_config_for_paths, is_read_action, preflight_action,
    ActionContext, ActionMeta, ActionPreflight, DashboardResponse, OrderResponse, OrdersResponse,
    ReadActionInput, ReadActionOutput, RedactedConfigResponse,
};
use gig_core::config::{Config, Paths};
use gig_core::Error as CoreError;
use serde::{Deserialize, Serialize};
use std::io::ErrorKind;
use std::path::PathBuf;
use time::OffsetDateTime;

#[derive(Clone)]
pub struct AppState {
    pub paths: Paths,
    pub config: Config,
    pub cwd: PathBuf,
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

#[derive(Debug, Deserialize)]
struct ExecuteActionRequest {
    #[serde(default)]
    input: ReadActionInput,
}

#[derive(Debug, Serialize)]
struct ActionPreviewResponse {
    action: &'static ActionMeta,
    preflight: ActionPreflight,
    executable: bool,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/", get(index))
        .route("/api/health", get(health))
        .route("/api/actions", get(actions_handler))
        .route("/api/actions/:id/preview", post(action_preview_handler))
        .route("/api/actions/:id/execute", post(action_execute_handler))
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

async fn actions_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<&'static [ActionMeta]>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    Ok(Json(actions::all()))
}

async fn action_preview_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(action_id): AxumPath<String>,
) -> Result<Json<ActionPreviewResponse>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    let action =
        action_meta_or_404(&action_id).ok_or_else(|| unknown_action_response(&action_id))?;
    let preflight = preflight_action(&action_id).map_err(core_error_response)?;
    Ok(Json(ActionPreviewResponse {
        action,
        preflight,
        executable: is_read_action(&action_id),
    }))
}

async fn action_execute_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(action_id): AxumPath<String>,
    body: Bytes,
) -> Result<Json<ReadActionOutput>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    action_meta_or_404(&action_id).ok_or_else(|| unknown_action_response(&action_id))?;
    if !is_read_action(&action_id) {
        return Err(unsupported_action_response(&action_id));
    }
    let request = parse_action_request(&body).map_err(|err| {
        api_error(
            StatusCode::BAD_REQUEST,
            "invalid_request",
            format!("invalid action request JSON: {err}"),
        )
    })?;
    let action_id: &'static str =
        read_action_id(&action_id).ok_or_else(|| unsupported_action_response(&action_id))?;
    let output = run_read_action(state, action_id, request.input).await?;
    Ok(Json(output))
}

async fn dashboard_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<DashboardResponse>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    let output = run_read_action(state, "dashboard.get", ReadActionInput::Empty).await?;
    match output {
        ReadActionOutput::Dashboard(dashboard) => Ok(Json(*dashboard)),
        other => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "action_output_mismatch",
            format!("dashboard.get returned {other:?}"),
        )),
    }
}

async fn orders_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<OrdersResponse>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    let output = run_read_action(
        state,
        "orders.list",
        ReadActionInput::OrdersList { status: None },
    )
    .await?;
    match output {
        ReadActionOutput::Orders(orders) => Ok(Json(*orders)),
        other => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "action_output_mismatch",
            format!("orders.list returned {other:?}"),
        )),
    }
}

async fn order_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    AxumPath(id): AxumPath<i64>,
) -> Result<Json<OrderResponse>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    let output = run_read_action(
        state,
        "orders.get_detail",
        ReadActionInput::OrderDetail {
            id_or_slug: Some(id.to_string()),
        },
    )
    .await?;
    match output {
        ReadActionOutput::Order(order) => Ok(Json(*order)),
        other => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "action_output_mismatch",
            format!("orders.get_detail returned {other:?}"),
        )),
    }
}

async fn config_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<RedactedConfigResponse>, Response> {
    if let Some(response) = unauthorized_response(&headers, &state) {
        return Err(response);
    }
    let output = run_read_action(state, "config.redacted.get", ReadActionInput::Empty).await?;
    match output {
        ReadActionOutput::Config(config) => Ok(Json(*config)),
        other => Err(api_error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "action_output_mismatch",
            format!("config.redacted.get returned {other:?}"),
        )),
    }
}

fn unauthorized_response(headers: &HeaderMap, state: &AppState) -> Option<Response> {
    if token_is_valid(headers, &state.token) {
        None
    } else {
        Some(api_error(
            StatusCode::UNAUTHORIZED,
            "unauthorized",
            "missing or invalid bearer token",
        ))
    }
}

fn action_meta_or_404(action_id: &str) -> Option<&'static ActionMeta> {
    actions::find_by_id(action_id)
}

fn unknown_action_response(action_id: &str) -> Response {
    api_error(
        StatusCode::NOT_FOUND,
        "unknown_action",
        format!("unknown action id: {action_id}"),
    )
}

fn read_action_id(action_id: &str) -> Option<&'static str> {
    match action_id {
        "dashboard.get" => Some("dashboard.get"),
        "orders.list" => Some("orders.list"),
        "orders.get_detail" => Some("orders.get_detail"),
        "config.redacted.get" => Some("config.redacted.get"),
        _ => None,
    }
}

fn unsupported_action_response(action_id: &str) -> Response {
    api_error(
        StatusCode::BAD_REQUEST,
        "unsupported_action",
        format!("action {action_id} is not executable through the read-only GUI API"),
    )
}

fn parse_action_request(body: &Bytes) -> Result<ExecuteActionRequest, serde_json::Error> {
    if body.is_empty() {
        return Ok(ExecuteActionRequest {
            input: ReadActionInput::Empty,
        });
    }
    serde_json::from_slice(body)
}

fn core_error_response(err: CoreError) -> Response {
    action_error_response(err)
}

async fn run_read_action(
    state: AppState,
    action_id: &'static str,
    input: ReadActionInput,
) -> Result<ReadActionOutput, Response> {
    let paths = state.paths;
    let config = state.config;
    let cwd = state.cwd;
    let now = OffsetDateTime::now_utc().unix_timestamp();
    if action_id == "config.redacted.get" {
        return run_blocking(move || match input {
            ReadActionInput::Empty => get_redacted_config_for_paths(&paths)
                .map(|output| ReadActionOutput::Config(Box::new(output))),
            other => Err(CoreError::Invalid(format!(
                "invalid input for config.redacted.get: {other:?}"
            ))),
        })
        .await;
    }

    run_blocking(move || {
        let conn = gig_core::db::open_readonly(&paths.db_file)?;
        let ctx = ActionContext::new(&conn, &paths, &config, &cwd, now);
        execute_read_action(&ctx, action_id, input)
    })
    .await
}

async fn run_blocking<T, F>(work: F) -> Result<T, Response>
where
    T: Send + 'static,
    F: FnOnce() -> gig_core::Result<T> + Send + 'static,
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
        .map_err(action_error_response)
}

fn action_error_response(err: CoreError) -> Response {
    let (status, code) = match &err {
        CoreError::Invalid(_) | CoreError::InvalidTransition { .. } | CoreError::Config(_) => {
            (StatusCode::BAD_REQUEST, "invalid_action_input")
        }
        CoreError::OrderNotFound(_) => (StatusCode::NOT_FOUND, "order_not_found"),
        CoreError::PathUnavailable(_, source) if source.kind() == ErrorKind::NotFound => {
            (StatusCode::NOT_FOUND, "path_not_found")
        }
        _ => (StatusCode::INTERNAL_SERVER_ERROR, "core_error"),
    };
    api_error(status, code, err.to_string())
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

    <section class="section">
      <h2>Actions</h2>
      <div class="card" id="actions">Loading…</div>
    </section>
  </main>

  <script>
    const hashParams = new URLSearchParams(location.hash.replace(/^#/, ''));
    const urlToken = hashParams.get('token');
    if (urlToken) {
      sessionStorage.setItem('gigGuiToken', urlToken);
      history.replaceState(null, '', location.pathname);
    }
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
      const [data, actions] = await Promise.all([api('/api/dashboard'), api('/api/actions')]);
      authState.textContent = 'localhost token ok';
      document.getElementById('metrics').innerHTML = [
        ['This month', money(data.summary.this_month_income)],
        ['Active orders', data.summary.orders_count],
        ['Pending payment', data.summary.pending_count],
      ].map(([label, value]) => `<div class="card"><div class="metric">${value}</div><div class="label">${label}</div></div>`).join('');
      document.getElementById('focus').innerHTML = rows(data.focus, 'order');
      document.getElementById('quotes').innerHTML = rows(data.quotes, 'quote');
      document.getElementById('actions').innerHTML = `<p class="muted">${actions.length} core action metadata entries available. Read action execution is wired.</p>`;
    }

    boot().catch(err => {
      authState.textContent = 'auth failed';
      authState.classList.add('error');
      document.getElementById('focus').innerHTML = `<p class="alert">${escapeHtml(err.message)}</p>`;
      document.getElementById('quotes').innerHTML = '<p class="muted">Unavailable</p>';
      document.getElementById('actions').innerHTML = '<p class="muted">Unavailable</p>';
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
                allow_insecure_http: false,
            },
        );
        config
    }

    fn test_router() -> (Router, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        paths.ensure_dirs().unwrap();
        gig_core::db::open(&paths.db_file).unwrap();
        test_config().save(&paths.config_file).unwrap();
        let app = router(AppState {
            paths,
            config: Config::default(),
            cwd: tmp.path().to_path_buf(),
            token: TEST_TOKEN.into(),
        });
        (app, tmp)
    }

    fn test_router_without_db() -> (Router, tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let paths = Paths::under_root(tmp.path());
        paths.ensure_dirs().unwrap();
        test_config().save(&paths.config_file).unwrap();
        let db_file = paths.db_file.clone();
        let app = router(AppState {
            paths,
            config: Config::default(),
            cwd: tmp.path().to_path_buf(),
            token: TEST_TOKEN.into(),
        });
        (app, tmp, db_file)
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

    async fn post_body(
        app: Router,
        uri: &str,
        token: Option<&str>,
        body: &'static str,
    ) -> Response {
        let mut request = Request::builder()
            .method("POST")
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(token) = token {
            request = request.header(AUTHORIZATION, format!("Bearer {token}"));
        }
        app.oneshot(request.body(Body::from(body)).unwrap())
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
    async fn dashboard_does_not_create_missing_database() {
        let (app, _tmp, db_file) = test_router_without_db();

        let response = get(app, "/api/dashboard", Some(TEST_TOKEN)).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert!(!db_file.exists());
        let body = json_body(response).await;
        assert_eq!(body["error"]["code"], "path_not_found");
    }

    #[tokio::test]
    async fn config_does_not_require_database() {
        let (app, _tmp, db_file) = test_router_without_db();

        let response = get(app, "/api/config", Some(TEST_TOKEN)).await;

        assert_eq!(response.status(), StatusCode::OK);
        assert!(!db_file.exists());
        let body = json_body(response).await;
        assert_eq!(body["delivery"]["short_link"]["token_set"], true);
    }

    #[tokio::test]
    async fn actions_endpoint_returns_core_catalog() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/actions", Some(TEST_TOKEN)).await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        let actions = body.as_array().unwrap();
        assert!(actions.iter().any(|action| action["id"] == "dashboard.get"));
        assert!(actions
            .iter()
            .any(|action| action["id"] == "delivery.package.send"));
        assert!(actions.iter().any(|action| action["id"] == "orders.delete"));
    }

    #[tokio::test]
    async fn actions_endpoint_rejects_missing_token() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/actions", None).await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn action_preview_requires_token() {
        let (app, _tmp) = test_router();

        let response = post_body(app, "/api/actions/orders.delete/preview", None, "{}").await;

        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn action_preview_returns_preflight_metadata() {
        let (app, _tmp) = test_router();

        let response = post_body(
            app,
            "/api/actions/orders.delete/preview",
            Some(TEST_TOKEN),
            "{}",
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["action"]["id"], "orders.delete");
        assert_eq!(body["preflight"]["confirmation"]["type"], "required");
        assert_eq!(body["executable"], false);
    }

    #[tokio::test]
    async fn read_action_execute_runs_dashboard() {
        let (app, _tmp) = test_router();

        let response = post_body(
            app,
            "/api/actions/dashboard.get/execute",
            Some(TEST_TOKEN),
            "{}",
        )
        .await;

        assert_eq!(response.status(), StatusCode::OK);
        let body = json_body(response).await;
        assert_eq!(body["type"], "dashboard");
        assert_eq!(body["data"]["summary"]["orders_count"], 0);
    }

    #[tokio::test]
    async fn mutating_action_execute_is_rejected() {
        let (app, _tmp) = test_router();

        let response = post_body(
            app,
            "/api/actions/orders.delete/execute",
            Some(TEST_TOKEN),
            "{}",
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_body(response).await;
        assert_eq!(body["error"]["code"], "unsupported_action");
    }

    #[tokio::test]
    async fn wrong_read_action_input_returns_bad_request() {
        let (app, _tmp) = test_router();

        let response = post_body(
            app,
            "/api/actions/dashboard.get/execute",
            Some(TEST_TOKEN),
            r#"{"input":{"type":"orders_list","status":"lead"}}"#,
        )
        .await;

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = json_body(response).await;
        assert_eq!(body["error"]["code"], "invalid_action_input");
    }

    #[tokio::test]
    async fn missing_order_returns_not_found() {
        let (app, _tmp) = test_router();

        let response = get(app, "/api/orders/999", Some(TEST_TOKEN)).await;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = json_body(response).await;
        assert_eq!(body["error"]["code"], "order_not_found");
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
}
