/// One explicitly compiled transition in a domain-owned state machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TransitionRule<S> {
    pub current: S,
    pub target: S,
}

impl<S> TransitionRule<S> {
    #[must_use]
    pub const fn new(current: S, target: S) -> Self {
        Self { current, target }
    }
}

/// Returns whether a transition is an idempotent no-op or is present in the
/// supplied static table. This is intentionally not a runtime workflow engine.
#[must_use]
pub fn transition_allowed<S: PartialEq>(
    current: &S,
    target: &S,
    rules: &[TransitionRule<S>],
) -> bool {
    current == target
        || rules
            .iter()
            .any(|rule| &rule.current == current && &rule.target == target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_static_or_idempotent_transitions_are_allowed() {
        let rules = [TransitionRule::new("new", "open")];
        assert!(transition_allowed(&"new", &"new", &rules));
        assert!(transition_allowed(&"new", &"open", &rules));
        assert!(!transition_allowed(&"open", &"new", &rules));
    }
}
