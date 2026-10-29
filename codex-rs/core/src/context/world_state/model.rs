//! Owns model context in both window modes. Ordinary windows retain model-switch behavior;
//! incremental windows record the base and refresh instructions without replacing their prefix.

use super::PreviousSectionState;
use super::SectionTransition;
use super::WorldStateHash;
use super::WorldStateSection;
use super::WorldStateUpdate;
use crate::context::BaseInstructionsFragment;
use crate::context::ContextualUserFragment;
use crate::context::InternalContextSource;
use crate::context::InternalModelContextFragment;
use crate::context::ModelInstructions;
use crate::context::ModelSwitchInstructions;
use serde::Deserialize;
use serde::Serialize;

/// Ordinary windows and older rollouts record only the model.
#[derive(Clone, Debug, Deserialize, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub(crate) enum ModelInstructionsSnapshot {
    WithInstructions {
        model: String,
        instructions: WorldStateHash,
    },
    ModelOnly(String),
}

/// Model identity and instructions relevant to the current window.
#[derive(Clone, Debug)]
pub(crate) struct ModelInstructionsState {
    /// The current tracking format is chosen by window mode, independently of saved state.
    snapshot: ModelInstructionsSnapshot,
    /// Ordinary-window fallback when the saved model snapshot is unavailable.
    previous_model: Option<String>,
    instructions: String,
}

impl ModelInstructionsState {
    pub(crate) fn new(model: &str, previous_model: Option<&str>, instructions: String) -> Self {
        Self {
            snapshot: ModelInstructionsSnapshot::ModelOnly(model.to_string()),
            previous_model: previous_model.map(str::to_string),
            instructions,
        }
    }

    pub(crate) fn for_incremental_tools(model: &str, instructions: String) -> Self {
        let instructions = BaseInstructionsFragment(instructions);
        let instructions_hash = WorldStateHash::from_fragment(&instructions);
        Self {
            snapshot: ModelInstructionsSnapshot::WithInstructions {
                model: model.to_string(),
                instructions: instructions_hash,
            },
            previous_model: None,
            instructions: instructions.0,
        }
    }
}

impl WorldStateSection for ModelInstructionsState {
    const ID: &'static str = "model";
    type Snapshot = ModelInstructionsSnapshot;

    fn matches_legacy_fragment(role: &str, text: &str) -> bool {
        role == "developer" && ModelSwitchInstructions::matches_text(text)
    }

    fn has_current_retained_fragment_matcher(&self) -> bool {
        matches!(self.snapshot, ModelInstructionsSnapshot::ModelOnly(_))
    }

    fn matches_retained_fragment(role: &str, text: &str) -> bool {
        Self::matches_legacy_fragment(role, text)
    }

    fn render_diff(
        &self,
        previous: PreviousSectionState<'_, Self::Snapshot>,
    ) -> SectionTransition<Self::Snapshot> {
        let (model, instructions_hash) = match &self.snapshot {
            ModelInstructionsSnapshot::ModelOnly(model) => (model, None),
            ModelInstructionsSnapshot::WithInstructions {
                model,
                instructions,
            } => (model, Some(instructions)),
        };
        if instructions_hash.is_some() && !matches!(previous, PreviousSectionState::Known(_)) {
            // Like tools, reassert full setup when the comparison state is missing.
            if self.instructions.is_empty() {
                return (Some(self.snapshot.clone()), Vec::new());
            }
            return (
                Some(self.snapshot.clone()),
                vec![WorldStateUpdate::prefix_item(ContextualUserFragment::into(
                    BaseInstructionsFragment(self.instructions.clone()),
                ))],
            );
        }
        let (previous_model, previous_instructions_hash) = match previous {
            PreviousSectionState::Known(ModelInstructionsSnapshot::ModelOnly(model)) => {
                (Some(model), None)
            }
            PreviousSectionState::Known(ModelInstructionsSnapshot::WithInstructions {
                model,
                instructions,
            }) if instructions_hash.is_some() => (Some(model), Some(instructions)),
            // Ordinary windows keep their fallback for an unrecognized incremental snapshot.
            PreviousSectionState::Known(ModelInstructionsSnapshot::WithInstructions { .. })
            | PreviousSectionState::Unknown
            | PreviousSectionState::Absent => (self.previous_model.as_ref(), None),
        };
        let model_changed = previous_model.is_some_and(|previous| previous != model);
        // Legacy model-only snapshots refresh instructions only when the model changes.
        let instructions_changed = instructions_hash.is_some_and(|current| {
            previous_instructions_hash.map_or(model_changed, |previous| previous != current)
        });
        if instructions_hash.is_some() && !model_changed && !instructions_changed {
            return (None, Vec::new());
        }
        let fragment: Option<Box<dyn ContextualUserFragment>> = if instructions_hash.is_some() {
            Some(Box::new(ModelInstructions {
                model: model_changed.then(|| model.clone()),
                instructions: instructions_changed.then(|| self.instructions.clone()),
            }))
        } else {
            (model_changed && !self.instructions.is_empty()).then(|| {
                Box::new(ModelSwitchInstructions::new(self.instructions.clone()))
                    as Box<dyn ContextualUserFragment>
            })
        };
        (
            Some(self.snapshot.clone()),
            WorldStateUpdate::optional_standalone_boxed_fragment(fragment),
        )
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
    ) -> SectionTransition<Self::Snapshot> {
        if matches!(previous, PreviousSectionState::Known(previous) if previous == &self.identity) {
            return (None, Vec::new());
        }
        (
            Some(self.identity.clone()),
            vec![WorldStateUpdate::fragment(
                InternalModelContextFragment::new(
                    InternalContextSource::from_static(Self::ID),
                    &self.identity,
                ),
            )],
        )
    }
}

#[cfg(test)]
#[path = "model_tests.rs"]
mod tests;
