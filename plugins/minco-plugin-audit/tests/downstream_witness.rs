//! Downstream compatibility witness (exact-head review 5060065907).
//!
//! An integration test is a separate crate: this file exhaustively
//! matches `AuditError` over EXACTLY the original two published
//! variants — no wildcard arm — so the moment a variant is added, this
//! witness (and every downstream 1.x consumer with the same match)
//! stops compiling. The integrity-conflict channel rides the stable
//! `MINCO-AUDIT-CONFLICT` code inside `Append` instead.

use minco_plugin_audit::{
    AUDIT_CONFLICT_CODE, AuditError, audit_conflict_error, is_audit_conflict,
};

#[test]
fn downstream_exhaustive_match_over_the_published_variants_compiles() {
    let render = |error: &AuditError| match error {
        // Exhaustive over the published variant set — no `_` arm. A new
        // variant breaks this match, which is the break this witness
        // exists to make visible.
        AuditError::InvalidEvent => "invalid",
        AuditError::Append(message) => {
            assert!(!message.is_empty());
            "append"
        }
    };
    assert_eq!(render(&AuditError::InvalidEvent), "invalid");
    assert_eq!(render(&AuditError::Append("boom".into())), "append");
}

#[test]
fn the_conflict_channel_rides_the_stable_append_code() {
    let error = audit_conflict_error();
    // It IS an Append variant (no new variant exists to match)…
    assert!(matches!(error, AuditError::Append(ref message)
        if message.starts_with(AUDIT_CONFLICT_CODE)));
    // …and the public helper recognizes it.
    assert!(is_audit_conflict(&error));
    // Ordinary append failures are NOT conflicts.
    assert!(!is_audit_conflict(&AuditError::Append(
        "database unavailable".into()
    )));
    assert!(!is_audit_conflict(&AuditError::InvalidEvent));
}
