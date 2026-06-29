use super::catalog::find_by_id;
use super::types::{ActionKind, ActionPreflight, ConfirmationPolicy, SideEffect};
use crate::{Error, Result};

pub fn preflight_action(action_id: &str) -> Result<ActionPreflight> {
    let meta = find_by_id(action_id)
        .ok_or_else(|| Error::Invalid(format!("unknown action id: {action_id}")))?;
    let mut preflight = ActionPreflight::ready(meta.confirmation);

    match meta.kind {
        ActionKind::Read => {}
        ActionKind::Mutate => preflight
            .warnings
            .push("This action may mutate local gig state.".into()),
        ActionKind::ExternalIo => preflight
            .warnings
            .push("This action may perform external network or upload I/O.".into()),
        ActionKind::Dangerous => preflight
            .warnings
            .push("This action can be destructive and requires explicit confirmation.".into()),
    }

    add_side_effect_warnings(&mut preflight, meta.side_effects);

    if matches!(meta.confirmation, ConfirmationPolicy::Required { .. }) {
        preflight
            .warnings
            .push("Confirmation is required before execution.".into());
    }

    Ok(preflight)
}

fn add_side_effect_warnings(preflight: &mut ActionPreflight, side_effects: &[SideEffect]) {
    if side_effects.contains(&SideEffect::WritesDatabase) {
        preflight
            .warnings
            .push("This action may write the local gig database.".into());
    }
    if side_effects.iter().any(|effect| {
        matches!(
            effect,
            SideEffect::WritesFiles | SideEffect::MovesFiles | SideEffect::DeletesFiles
        )
    }) {
        preflight
            .warnings
            .push("This action may change local files.".into());
    }
    if side_effects.iter().any(|effect| {
        matches!(
            effect,
            SideEffect::ExternalNetwork | SideEffect::UploadsFiles
        )
    }) {
        preflight
            .warnings
            .push("This action may use external network or upload I/O.".into());
    }
    if side_effects.contains(&SideEffect::OpensEditor) {
        preflight
            .warnings
            .push("This action may open an editor.".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_action_preflight_has_no_confirmation() {
        let preflight = preflight_action("dashboard.get").unwrap();

        assert_eq!(preflight.confirmation, ConfirmationPolicy::None);
        assert!(preflight.warnings.is_empty());
        assert_eq!(preflight.blocked_reason, None);
    }

    #[test]
    fn mutating_action_preflight_mentions_database_writes() {
        let preflight = preflight_action("delivery.package.check").unwrap();

        assert_eq!(preflight.confirmation, ConfirmationPolicy::None);
        assert!(preflight
            .warnings
            .iter()
            .any(|warning| warning.contains("local gig state")));
        assert!(preflight
            .warnings
            .iter()
            .any(|warning| warning.contains("local gig database")));
    }

    #[test]
    fn dangerous_action_preflight_requires_confirmation() {
        let preflight = preflight_action("orders.delete").unwrap();

        assert!(matches!(
            preflight.confirmation,
            ConfirmationPolicy::Required { .. }
        ));
        assert!(preflight
            .warnings
            .iter()
            .any(|warning| warning.contains("destructive")));
        assert!(preflight
            .warnings
            .iter()
            .any(|warning| warning.contains("Confirmation")));
    }

    #[test]
    fn unknown_action_preflight_fails() {
        let err = preflight_action("missing.action").unwrap_err().to_string();

        assert!(err.contains("unknown action id"));
    }
}
