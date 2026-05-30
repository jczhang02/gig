use super::types::{
    ActionField, ActionFieldKind, ActionId, ActionKind, ActionMeta, ActionOutputKind,
    ConfirmationPolicy, SideEffect,
};

const PROJECT_TYPES: &[&str] = &[
    "automation_script",
    "data_processing",
    "crawler",
    "cv_ml",
    "frontend_web",
    "document_pdf",
    "research_writing",
    "custom",
];

const ORDER_STATUSES: &[&str] = &[
    "lead",
    "negotiating",
    "accepted",
    "plan_ready",
    "plan_approved",
    "in_progress",
    "ready_to_deliver",
    "delivered",
    "revision",
    "paid",
    "archived",
    "cancelled",
];

const EMPTY_FIELDS: &[ActionField] = &[];
const ORDER_ID_OPTIONAL: &[ActionField] = &[ActionField::optional(
    "id",
    "Order id or slug",
    ActionFieldKind::String,
)];
const ORDER_ID_REQUIRED: &[ActionField] = &[ActionField::required(
    "id",
    "Order id or slug",
    ActionFieldKind::String,
)];
const DASHBOARD_FIELDS: &[ActionField] = &[
    ActionField::optional(
        "status",
        "Status",
        ActionFieldKind::Enum {
            values: ORDER_STATUSES,
        },
    ),
    ActionField::optional("all", "Include terminal orders", ActionFieldKind::Boolean),
];
const ORDER_CREATE_FIELDS: &[ActionField] = &[
    ActionField::required("title", "Title", ActionFieldKind::String),
    ActionField::optional("slug", "Slug", ActionFieldKind::String),
    ActionField::optional("quoted_price", "Quoted price", ActionFieldKind::Decimal),
    ActionField::optional("final_price", "Final price", ActionFieldKind::Decimal),
    ActionField::optional("lead", "Create as lead", ActionFieldKind::Boolean),
];
const PRICE_FIELDS: &[ActionField] = &[
    ActionField::optional("id", "Order id or slug", ActionFieldKind::String),
    ActionField::required("amount", "Amount", ActionFieldKind::Decimal),
    ActionField::optional("reason", "Reason", ActionFieldKind::String),
];
const CHANGE_FIELDS: &[ActionField] = &[
    ActionField::optional("id", "Order id or slug", ActionFieldKind::String),
    ActionField::required("message", "Message", ActionFieldKind::String),
    ActionField::optional("delta", "Price delta", ActionFieldKind::Decimal),
];
const NOTE_FIELDS: &[ActionField] = &[
    ActionField::optional("id", "Order id or slug", ActionFieldKind::String),
    ActionField::required("text", "Note", ActionFieldKind::String),
];
const TAG_FIELDS: &[ActionField] = &[
    ActionField::optional("id", "Order id or slug", ActionFieldKind::String),
    ActionField::repeated("tags", "Tags", ActionFieldKind::String),
];
const CUT_FIELDS: &[ActionField] = &[
    ActionField::optional("id", "Order id or slug", ActionFieldKind::String),
    ActionField::required("ratio", "Cut ratio", ActionFieldKind::Decimal),
];
const STATUS_FIELDS: &[ActionField] = &[
    ActionField::optional("id", "Order id or slug", ActionFieldKind::String),
    ActionField::required(
        "status",
        "Status",
        ActionFieldKind::Enum {
            values: ORDER_STATUSES,
        },
    ),
];
const PACKAGE_FIELDS: &[ActionField] = &[
    ActionField::required("id_or_slug", "Order id or slug", ActionFieldKind::String),
    ActionField::required("delivery_date", "Delivery date", ActionFieldKind::String),
    ActionField::required("delivery_dir", "Delivery directory", ActionFieldKind::Path),
];
const PACKAGE_SEND_FIELDS: &[ActionField] = &[
    ActionField::required("id_or_slug", "Order id or slug", ActionFieldKind::String),
    ActionField::required("delivery_date", "Delivery date", ActionFieldKind::String),
    ActionField::required("delivery_dir", "Delivery directory", ActionFieldKind::Path),
    ActionField::optional("uploader", "Uploader", ActionFieldKind::String),
];
const ARTIFACT_SEND_FIELDS: &[ActionField] = &[
    ActionField::required("id_or_slug", "Order id or slug", ActionFieldKind::String),
    ActionField::repeated("files", "Files", ActionFieldKind::PathList),
    ActionField::optional("uploader", "Uploader", ActionFieldKind::String),
];
const QUOTE_CREATE_FIELDS: &[ActionField] = &[
    ActionField::required("slug", "Slug", ActionFieldKind::String),
    ActionField::required("title", "Title", ActionFieldKind::String),
    ActionField::required(
        "project_type",
        "Project type",
        ActionFieldKind::Enum {
            values: PROJECT_TYPES,
        },
    ),
    ActionField::required("summary", "Summary", ActionFieldKind::String),
];
const QUOTE_PRICE_FIELDS: &[ActionField] = &[
    ActionField::required(
        "id_or_slug",
        "Quote draft id or slug",
        ActionFieldKind::String,
    ),
    ActionField::required("min", "Minimum quote", ActionFieldKind::Decimal),
    ActionField::required("recommended", "Recommended quote", ActionFieldKind::Decimal),
    ActionField::required("max", "Maximum quote", ActionFieldKind::Decimal),
];
const QUOTE_ACCEPT_FIELDS: &[ActionField] = &[
    ActionField::required(
        "id_or_slug",
        "Quote draft id or slug",
        ActionFieldKind::String,
    ),
    ActionField::required("project_dir", "Project directory", ActionFieldKind::Path),
    ActionField::optional("my_cut_ratio", "My cut ratio", ActionFieldKind::Decimal),
];
const REASON_FIELDS: &[ActionField] = &[
    ActionField::required("id_or_slug", "Id or slug", ActionFieldKind::String),
    ActionField::required("reason", "Reason", ActionFieldKind::String),
];
const SOURCE_CREATE_FIELDS: &[ActionField] = &[
    ActionField::required("name", "Name", ActionFieldKind::String),
    ActionField::required("cut_ratio", "Cut ratio", ActionFieldKind::Decimal),
    ActionField::optional("notes", "Notes", ActionFieldKind::String),
];
const CONFIG_KEY_FIELDS: &[ActionField] = &[ActionField::required(
    "key",
    "Config key",
    ActionFieldKind::String,
)];
const CONFIG_SET_FIELDS: &[ActionField] = &[
    ActionField::required("key", "Config key", ActionFieldKind::String),
    ActionField::required("value", "Value", ActionFieldKind::String),
];

macro_rules! path {
    ($($segment:literal),+ $(,)?) => {
        &[$($segment),+]
    };
}

macro_rules! effects {
    () => {
        &[]
    };
    ($($effect:ident),+ $(,)?) => {
        &[$(SideEffect::$effect),+]
    };
}

macro_rules! action {
    (
        $id:literal,
        [$($path:literal),+],
        $label:literal,
        $description:literal,
        $kind:ident,
        $fields:expr,
        $confirmation:expr,
        [$($effect:ident),*],
        $output:ident,
        json: $json:expr,
        workflow: $workflow:expr $(,)?
    ) => {
        ActionMeta {
            id: ActionId::new($id),
            cli_path: path![$($path),+],
            label: $label,
            description: $description,
            kind: ActionKind::$kind,
            fields: $fields,
            confirmation: $confirmation,
            side_effects: effects![$($effect),*],
            output: ActionOutputKind::$output,
            json_supported: $json,
            workflow_critical: $workflow,
        }
    };
}

const NONE: ConfirmationPolicy = ConfirmationPolicy::None;
const CONFIRM_STATUS: ConfirmationPolicy = ConfirmationPolicy::Required {
    prompt: "Force-set this Order status?",
};
const CONFIRM_ARCHIVE: ConfirmationPolicy = ConfirmationPolicy::Required {
    prompt: "Archive this Order and move or delete local files?",
};
const CONFIRM_DELETE: ConfirmationPolicy = ConfirmationPolicy::Required {
    prompt: "Delete this Order from the database?",
};
const CONFIRM_IMPORT_FORCE: ConfirmationPolicy = ConfirmationPolicy::Required {
    prompt: "Import projects and possibly replace existing records?",
};
const CONFIRM_CONFIG_EDIT: ConfirmationPolicy = ConfirmationPolicy::Required {
    prompt: "Open or mutate local configuration?",
};

static ACTIONS: &[ActionMeta] = &[
    action!(
        "orders.create",
        ["new"],
        "Create Order",
        "Create a new Order or lead.",
        Mutate,
        ORDER_CREATE_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "dashboard.get",
        ["ls"],
        "Dashboard",
        "Read the workflow decision board used by gig ls.",
        Read,
        DASHBOARD_FIELDS,
        NONE,
        [ReadsDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "orders.get_detail",
        ["show"],
        "Show Order",
        "Read a single Order with related workflow data.",
        Read,
        ORDER_ID_OPTIONAL,
        NONE,
        [ReadsDatabase],
        Json,
        json: true,
        workflow: false,
    ),
    action!(
        "orders.price.update",
        ["price"],
        "Update Price",
        "Update an Order final price.",
        Mutate,
        PRICE_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.requirement_change.add",
        ["change"],
        "Add Requirement Change",
        "Record a requirement change on an Order.",
        Mutate,
        CHANGE_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.notes.append",
        ["note"],
        "Append Note",
        "Append a note to an Order.",
        Mutate,
        NOTE_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.tags.add",
        ["tag"],
        "Add Tags",
        "Attach tags to an Order.",
        Mutate,
        TAG_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.cut_ratio.update",
        ["cut"],
        "Update Cut Ratio",
        "Update the developer cut ratio for an Order.",
        Mutate,
        CUT_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.status.set",
        ["status"],
        "Set Status",
        "Force-set an Order status after confirmation.",
        Dangerous,
        STATUS_FIELDS,
        CONFIRM_STATUS,
        [WritesDatabase],
        Json,
        json: false,
        workflow: true,
    ),
    action!(
        "orders.payment.mark_paid",
        ["paid"],
        "Mark Paid",
        "Mark an Order as paid.",
        Mutate,
        ORDER_ID_OPTIONAL,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.archive",
        ["archive"],
        "Archive Order",
        "Archive an Order and optionally move or purge project files.",
        Dangerous,
        ORDER_ID_OPTIONAL,
        CONFIRM_ARCHIVE,
        [WritesDatabase, MovesFiles, DeletesFiles],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "leads.promote",
        ["lead", "promote"],
        "Promote Lead",
        "Promote a lead to negotiating.",
        Mutate,
        ORDER_ID_REQUIRED,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "leads.drop",
        ["lead", "drop"],
        "Drop Lead",
        "Cancel a lead.",
        Mutate,
        ORDER_ID_REQUIRED,
        NONE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "leads.list",
        ["lead", "ls"],
        "List Leads",
        "List all lead Orders.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.dev_path.get",
        ["cd"],
        "Get Dev Path",
        "Print an Order development path.",
        Read,
        ORDER_ID_OPTIONAL,
        NONE,
        [ReadsDatabase],
        FilePath,
        json: false,
        workflow: false,
    ),
    action!(
        "system.doctor.run",
        ["doctor"],
        "Run Doctor",
        "Run local consistency checks.",
        Mutate,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase, ReadsFiles, WritesDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "stats.get",
        ["stats"],
        "Get Stats",
        "Read income statistics.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.export",
        ["export"],
        "Export Orders",
        "Export Orders to CSV or JSON.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase, WritesFiles],
        File,
        json: false,
        workflow: false,
    ),
    action!(
        "clients.list",
        ["client", "ls"],
        "List Clients",
        "List clients.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "clients.get_detail",
        ["client", "show"],
        "Show Client",
        "Read one client and their Orders.",
        Read,
        &[ActionField::required("id", "Client id", ActionFieldKind::Integer)],
        NONE,
        [ReadsDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "templates.list",
        ["template", "ls"],
        "List Templates",
        "List embedded and user templates.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsFiles],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "templates.get",
        ["template", "show"],
        "Show Template",
        "Print a template.",
        Read,
        &[ActionField::required("name", "Template name", ActionFieldKind::String)],
        NONE,
        [ReadsFiles],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "templates.save_user_copy",
        ["template", "edit"],
        "Edit Template",
        "Prepare a user template copy for editing.",
        Dangerous,
        &[ActionField::required("name", "Template name", ActionFieldKind::String)],
        CONFIRM_CONFIG_EDIT,
        [ReadsFiles, WritesFiles, OpensEditor],
        FilePath,
        json: false,
        workflow: false,
    ),
    action!(
        "config.get",
        ["config", "get"],
        "Get Config",
        "Read a config value.",
        Read,
        CONFIG_KEY_FIELDS,
        NONE,
        [ReadsFiles],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "config.set",
        ["config", "set"],
        "Set Config",
        "Update a config value.",
        Mutate,
        CONFIG_SET_FIELDS,
        CONFIRM_CONFIG_EDIT,
        [ReadsFiles, WritesFiles],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "config.file.get_path",
        ["config", "edit"],
        "Edit Config",
        "Return the config file path for editor-based adapters.",
        Dangerous,
        EMPTY_FIELDS,
        CONFIRM_CONFIG_EDIT,
        [ReadsFiles, WritesFiles, OpensEditor],
        FilePath,
        json: false,
        workflow: false,
    ),
    action!(
        "projects.import",
        ["import"],
        "Import Projects",
        "Import existing project directories into gig.",
        Dangerous,
        &[ActionField::repeated("paths", "Paths", ActionFieldKind::PathList)],
        CONFIRM_IMPORT_FORCE,
        [ReadsFiles, WritesDatabase, MovesFiles],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "orders.delete",
        ["delete"],
        "Delete Order",
        "Delete an Order from the database.",
        Dangerous,
        ORDER_ID_REQUIRED,
        CONFIRM_DELETE,
        [WritesDatabase],
        Json,
        json: false,
        workflow: false,
    ),
    action!(
        "system.backup.create",
        ["backup"],
        "Create Backup",
        "Snapshot the database to the backups directory.",
        Mutate,
        EMPTY_FIELDS,
        NONE,
        [ReadsFiles, WritesFiles],
        FilePath,
        json: false,
        workflow: false,
    ),
    action!(
        "cli.completion.generate",
        ["completion"],
        "Generate Completion",
        "Generate a shell completion script.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [PrintsCompletion],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "sources.create",
        ["source", "add"],
        "Create Source",
        "Create a source with commission metadata.",
        Mutate,
        SOURCE_CREATE_FIELDS,
        NONE,
        [WritesDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "sources.list",
        ["source", "ls"],
        "List Sources",
        "List sources.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase],
        Text,
        json: false,
        workflow: false,
    ),
    action!(
        "quotes.create",
        ["quote", "new"],
        "Create Quote Draft",
        "Record a pre-acceptance Quote Draft.",
        Mutate,
        QUOTE_CREATE_FIELDS,
        NONE,
        [WritesDatabase, WritesFiles],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "quotes.get",
        ["quote", "show"],
        "Show Quote Draft",
        "Read a Quote Draft.",
        Read,
        &[ActionField::required("id_or_slug", "Quote draft id or slug", ActionFieldKind::String)],
        NONE,
        [ReadsDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "quotes.list",
        ["quote", "list"],
        "List Quote Drafts",
        "List Quote Drafts.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [ReadsDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "quotes.price",
        ["quote", "price"],
        "Price Quote Draft",
        "Record a quote range on a Quote Draft.",
        Mutate,
        QUOTE_PRICE_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "quotes.mark_sent",
        ["quote", "mark-sent"],
        "Mark Quote Sent",
        "Mark a priced Quote Draft as sent.",
        Mutate,
        &[ActionField::required("id_or_slug", "Quote draft id or slug", ActionFieldKind::String)],
        NONE,
        [WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "quotes.accept",
        ["quote", "accept"],
        "Accept Quote Draft",
        "Accept a Quote Draft into an Order and register workflow paths.",
        Mutate,
        QUOTE_ACCEPT_FIELDS,
        NONE,
        [WritesDatabase, ReadsFiles],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "quotes.drop",
        ["quote", "drop"],
        "Drop Quote Draft",
        "Drop a Quote Draft with a reason.",
        Mutate,
        REASON_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "workflow.plan.ready",
        ["plan", "ready"],
        "Plan Ready",
        "Mark workflow-created plan files ready for review.",
        Mutate,
        ORDER_ID_REQUIRED,
        NONE,
        [ReadsFiles, WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "workflow.plan.approve",
        ["plan", "approve"],
        "Approve Plan",
        "Approve a ready plan.",
        Mutate,
        ORDER_ID_REQUIRED,
        NONE,
        [WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "workflow.plan.reject",
        ["plan", "reject"],
        "Reject Plan",
        "Reject a ready plan and record the reason.",
        Mutate,
        REASON_FIELDS,
        NONE,
        [WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "workflow.work.start",
        ["work", "start"],
        "Start Work",
        "Start work after plan approval.",
        Mutate,
        ORDER_ID_REQUIRED,
        NONE,
        [WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "workflow.acceptance.check",
        ["acceptance", "check"],
        "Check Acceptance",
        "Check workflow-created acceptance evidence.",
        Read,
        ORDER_ID_REQUIRED,
        NONE,
        [ReadsDatabase, ReadsFiles],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "workflow.acceptance.complete",
        ["acceptance", "complete"],
        "Complete Acceptance",
        "Complete acceptance and mark the Order ready to deliver.",
        Mutate,
        ORDER_ID_REQUIRED,
        NONE,
        [ReadsFiles, WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "delivery.package.check",
        ["package", "check"],
        "Check Client Package",
        "Validate a workflow-created Client Package without uploading.",
        Read,
        PACKAGE_FIELDS,
        NONE,
        [ReadsFiles, ReadsDatabase, WritesDatabase],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "delivery.package.send",
        ["package", "send"],
        "Send Client Package",
        "Validate and upload a Client Package, then mark sent.",
        ExternalIo,
        PACKAGE_SEND_FIELDS,
        ConfirmationPolicy::Required {
            prompt: "Upload this Client Package and mark the Order delivered?",
        },
        [ReadsFiles, WritesDatabase, ExternalNetwork, UploadsFiles],
        Json,
        json: true,
        workflow: true,
    ),
    action!(
        "delivery.artifacts.send",
        ["artifact", "send"],
        "Send Delivery Artifacts",
        "Upload standalone Delivery Artifacts without changing Order status.",
        ExternalIo,
        ARTIFACT_SEND_FIELDS,
        ConfirmationPolicy::Required {
            prompt: "Upload these Delivery Artifacts?",
        },
        [ReadsFiles, WritesDatabase, ExternalNetwork, UploadsFiles],
        Json,
        json: true,
        workflow: false,
    ),
    action!(
        "gui.start",
        ["gui"],
        "Start GUI",
        "Start the local read-only GUI companion.",
        Read,
        EMPTY_FIELDS,
        NONE,
        [StartsLocalServer],
        LocalServer,
        json: false,
        workflow: false,
    ),
];

pub fn all() -> &'static [ActionMeta] {
    ACTIONS
}

pub fn find_by_id(id: &str) -> Option<&'static ActionMeta> {
    ACTIONS.iter().find(|action| action.id.as_str() == id)
}

pub fn find_by_cli_path(path: &[&str]) -> Option<&'static ActionMeta> {
    ACTIONS.iter().find(|action| action.cli_path == path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn catalog_has_unique_ids_and_cli_paths() {
        let mut ids = BTreeSet::new();
        let mut cli_paths = BTreeSet::new();

        for action in all() {
            assert!(ids.insert(action.id.as_str()), "duplicate id {}", action.id);
            assert!(
                cli_paths.insert(action.cli_path.join(" ")),
                "duplicate cli path {:?}",
                action.cli_path
            );
        }
    }

    #[test]
    fn catalog_exposes_phase_b_contracts() {
        let create = find_by_id("orders.create").unwrap();
        assert_eq!(create.cli_path, ["new"]);
        assert!(create.fields.iter().any(|field| field.name == "title"));

        let delete = find_by_id("orders.delete").unwrap();
        assert_eq!(delete.kind, ActionKind::Dangerous);
        assert!(matches!(
            delete.confirmation,
            ConfirmationPolicy::Required { .. }
        ));

        let send = find_by_id("delivery.package.send").unwrap();
        assert_eq!(send.kind, ActionKind::ExternalIo);
        assert!(send.side_effects.contains(&SideEffect::UploadsFiles));

        let dashboard = find_by_id("dashboard.get").unwrap();
        assert!(dashboard.workflow_critical);
        assert!(dashboard.json_supported);
    }

    #[test]
    fn catalog_includes_current_cli_command_paths() {
        let cli_source = include_str!("../../../gig-cli/src/cli.rs");
        let expected = expected_cli_paths(cli_source);
        let catalog_paths = all()
            .iter()
            .map(|action| action.cli_path.join(" "))
            .collect::<BTreeSet<_>>();

        for path in &expected {
            assert!(
                catalog_paths.contains(path),
                "missing action metadata for cli path `{path}`"
            );
        }
    }

    fn expected_cli_paths(source: &str) -> BTreeSet<String> {
        let nested = [
            ("Lead", "LeadCommand"),
            ("Package", "PackageCommand"),
            ("Artifact", "ArtifactCommand"),
            ("Acceptance", "AcceptanceCommand"),
            ("Plan", "PlanCommand"),
            ("Work", "WorkCommand"),
            ("Quote", "QuoteCommand"),
            ("Client", "ClientCommand"),
            ("Template", "TemplateCommand"),
            ("Config", "ConfigCommand"),
            ("Source", "SourceCommand"),
        ]
        .into_iter()
        .collect::<std::collections::BTreeMap<_, _>>();

        enum_variants(source, "Command")
            .into_iter()
            .flat_map(|variant| {
                let command = clap_name(&variant);
                if let Some(enum_name) = nested.get(variant.as_str()) {
                    enum_variants(source, enum_name)
                        .into_iter()
                        .map(|subcommand| format!("{command} {}", clap_name(&subcommand)))
                        .collect::<Vec<_>>()
                } else {
                    vec![command]
                }
            })
            .collect()
    }

    fn enum_variants(source: &str, enum_name: &str) -> Vec<String> {
        let marker = format!("pub enum {enum_name} {{");
        let start = source
            .find(&marker)
            .unwrap_or_else(|| panic!("missing {enum_name}"));
        let body = &source[start + marker.len()..];
        body.lines()
            .take_while(|line| !line.trim_start().starts_with('}'))
            .filter_map(|line| {
                let line = line.trim();
                if line.is_empty() || line.starts_with("///") || line.starts_with("#[") {
                    return None;
                }
                let name = line
                    .split(['(', ',', ' '])
                    .next()
                    .filter(|name| name.chars().all(|ch| ch.is_ascii_alphanumeric()))?;
                Some(name.to_string())
            })
            .collect()
    }

    fn clap_name(variant: &str) -> String {
        let mut output = String::new();
        for (index, ch) in variant.chars().enumerate() {
            if ch.is_ascii_uppercase() {
                if index > 0 {
                    output.push('-');
                }
                output.push(ch.to_ascii_lowercase());
            } else {
                output.push(ch);
            }
        }
        output
    }
}
