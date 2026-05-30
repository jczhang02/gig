use clap::Parser;
use gig_core::Error;
use serde_json::json;

mod cli;
mod commands;
mod dispatch;
mod ui;

fn main() {
    let args = cli::Cli::parse();
    let json_errors = args.wants_json_errors();
    if let Err(e) = dispatch::run(args) {
        if json_errors {
            print_json_error(&e);
        } else {
            eprintln!("error: {e}");
        }
        std::process::exit(1);
    }
}

fn print_json_error(error: &Error) {
    let output = json!({
        "status": "error",
        "error": {
            "code": workflow_error_code(error),
            "message": error.to_string(),
        }
    });
    eprintln!("{output}");
}

fn workflow_error_code(error: &Error) -> &'static str {
    match error {
        Error::InvalidTransition { .. } => "invalid_transition",
        Error::Invalid(message) => classify_invalid_error(message),
        Error::TomlDe(_) => "missing_required_field",
        Error::Io(_) | Error::PathUnavailable(_, _) => "missing_workflow_file",
        Error::OrderNotFound(_) => "missing_required_field",
        Error::Db(_) | Error::Migration(_) | Error::Config(_) | Error::TomlSer(_) => {
            "missing_required_field"
        }
    }
}

fn classify_invalid_error(message: &str) -> &'static str {
    let lower = message.to_ascii_lowercase();
    if lower.contains("unknown project type") {
        "invalid_project_type"
    } else if lower.contains("unsafe package path") {
        "unsafe_package_path"
    } else if lower.contains("client-package.zip")
        || lower.contains("package artifact")
        || lower.contains("package zip entry")
    {
        "missing_package_artifact"
    } else if lower.contains("acceptance headings missing")
        || lower.contains("acceptance item incomplete")
    {
        "acceptance_incomplete"
    } else if lower.contains("missing order_workflow") || lower.contains("missing expected") {
        "legacy_workflow_metadata_missing"
    } else if lower.contains("missing workflow-created") {
        "missing_workflow_file"
    } else if lower.contains("expected order")
        || lower.contains("cannot mark plan ready")
        || lower.contains("invalid transition")
        || lower.contains("requires ready_to_deliver")
        || lower.contains("must be prepared or validated")
        || lower.contains("must be validated")
    {
        "invalid_transition"
    } else {
        "missing_required_field"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn package_zip_entry_errors_are_package_artifact_errors() {
        let error = Error::Invalid("missing package zip entry: DELIVERY_CLIENT.html".to_string());

        assert_eq!(workflow_error_code(&error), "missing_package_artifact");
    }
}
