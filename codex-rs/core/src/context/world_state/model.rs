use super::PreviousSectionState;
use super::WorldStateSection;
use crate::context::ContextualUserFragment;
use crate::context::InternalContextSource;
use crate::context::InternalModelContextFragment;
use crate::context::ModelSwitchInstructions;

/// Model identity and the instructions needed when that identity changes.
#[derive(Clone, Debug)]
pub(crate) struct ModelInstructionsState {
    model: String,
    previous_model: Option<String>,
    instructions: String,
}

impl ModelInstructionsState {
    pub(crate) fn new(model: &str, previous_model: Option<&str>, instructions: String) -> Self {
        Self {
            model: model.to_string(),
            previous_model: previous_model.map(str::to_string),
            instructions,
        }
    }
}

impl WorldStateSection for ModelInstructionsState {
    const ID: &'static str = "model";
    type Snapshot = String;

    fn snapshot(&self) -> Self::Snapshot {
        self.model.clone()
    }

    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        role == "developer" && ModelSwitchInstructions::matches_text(text)
    }

    fn has_retained_fragment_matcher() -> bool {
        true
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        Self::matches_legacy_fragment(role, text)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> Option<Box<dyn ContextualUserFragment>> {
        let model_changed = match previous {
            PreviousSectionState::Known(previous) => previous != &self.model,
            PreviousSectionState::Unknown | PreviousSectionState::Absent => self
                .previous_model
                .as_deref()
                .is_some_and(|previous| previous != self.model),
        };

        (model_changed && !self.instructions.is_empty()).then(|| {
            Box::new(ModelSwitchInstructions::new(self.instructions.clone()))
                as Box<dyn ContextualUserFragment>
        })
    }
}

#[derive(Clone, Debug)]
pub(crate) struct ModelIdentityState {
    identity: String,
}

impl ModelIdentityState {
    pub(crate) fn new(model: &str, display_name: &str, reasoning_effort: Option<&str>) -> Self {
        let model = model.chars().take(128).collect::<String>();
        let display_name = display_name.chars().take(128).collect::<String>();
        let identity = match reasoning_effort {
            Some(effort) => {
                let effort = effort.chars().take(64).collect::<String>();
                format!(
                    "Current model: {display_name} (ID: {model}); reasoning effort: {effort}. This is the runtime selection for this step."
                )
            }
            None => format!(
                "Current model: {display_name} (ID: {model}). This is the runtime selection for this step."
            ),
        };
        Self { identity }
    }
}

impl WorldStateSection for ModelIdentityState {
    const ID: &'static str = "model_identity";
    type Snapshot = String;

    fn snapshot(&self) -> Self::Snapshot {
        self.identity.clone()
    }

    fn has_retained_fragment_matcher() -> bool {
        true
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        role == "user"
            && text
                .trim()
                .starts_with("<codex_internal_context source=\"model_identity\">")
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> Option<Box<dyn ContextualUserFragment>> {
        if matches!(previous, PreviousSectionState::Known(previous) if previous == &self.identity) {
            return None;
        }
        Some(Box::new(InternalModelContextFragment::new(
            InternalContextSource::from_static(Self::ID),
            &self.identity,
        )))
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
