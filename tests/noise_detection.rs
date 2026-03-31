//! Tests for noise detection heuristics — ensuring domain types are not
//! incorrectly classified as noise while infrastructure types are.

mod common;

use kodex::query::commands::noise::compute_noise_patterns;

fn all_noise_patterns(index: &kodex::model::ArchivedKodexIndex) -> Vec<String> {
    compute_noise_patterns(index, 15)
        .into_iter()
        .flat_map(|c| c.patterns)
        .collect()
}

fn noise_patterns_by_category(
    index: &kodex::model::ArchivedKodexIndex,
    category: &str,
) -> Vec<String> {
    compute_noise_patterns(index, 15)
        .into_iter()
        .filter(|c| c.label == category)
        .flat_map(|c| c.patterns)
        .collect()
}

// ── Hub utilities: domain types should NOT be flagged ──────────────────────

#[test]
fn hub_noise_does_not_flag_domain_types() {
    let ti = common::build_and_load_index(common::make_hub_noise_docs());
    let patterns = all_noise_patterns(ti.index());

    // Domain types with moderate ref counts should NOT be noise
    assert!(
        !patterns.iter().any(|p| p == "RequestContext"),
        "RequestContext (~200 refs, 25 modules) should not be classified as noise.\n\
         Got patterns: {patterns:?}"
    );
    assert!(
        !patterns.iter().any(|p| p == "AuthContext"),
        "AuthContext (~140 refs, 20 modules) should not be classified as noise.\n\
         Got patterns: {patterns:?}"
    );
}

#[test]
fn hub_noise_flags_infrastructure_types() {
    let ti = common::build_and_load_index(common::make_hub_noise_docs());
    let patterns = all_noise_patterns(ti.index());

    // Infrastructure types with very high ref counts SHOULD be noise
    assert!(
        patterns.iter().any(|p| p == "DatabaseDriver"),
        "DatabaseDriver (~585 refs, 45 modules) should be classified as noise.\n\
         Got patterns: {patterns:?}"
    );
    assert!(
        patterns.iter().any(|p| p == "StringUtils"),
        "StringUtils (~400 refs, 40 modules) should be classified as noise.\n\
         Got patterns: {patterns:?}"
    );
}

// ── Effect plumbing: should emit method-level patterns, not type-level ─────

#[test]
fn effect_plumbing_emits_method_not_owner() {
    let ti = common::build_and_load_index(common::make_hub_noise_docs());
    let patterns = all_noise_patterns(ti.index());

    // If userId is detected as effect plumbing, the pattern should be
    // "RequestContext.userId" (method-qualified), NOT "RequestContext" (type-level).
    // This ensures the owning type remains searchable.
    let rc_patterns: Vec<&String> = patterns.iter().filter(|p| p.contains("RequestContext")).collect();
    assert!(
        !rc_patterns.is_empty(),
        "Expected at least one noise pattern involving RequestContext.userId as effect plumbing.\n\
         Got patterns: {patterns:?}"
    );
    for p in &rc_patterns {
        assert!(
            p.contains('.') && p.contains("userId"),
            "Noise pattern containing 'RequestContext' should be method-qualified \
             (e.g., 'RequestContext.userId'), not type-level. Got: '{p}'"
        );
    }
}

// ── Small codebases should not produce hub noise ───────────────────────────

#[test]
fn small_codebase_no_hub_noise() {
    // The billing fixture has 1 module with ~5 symbols — no hub noise possible
    let ti = common::build_and_load_index(common::make_billing_test_docs());
    let hub_patterns = noise_patterns_by_category(ti.index(), "Hub utilities");
    assert!(
        hub_patterns.is_empty(),
        "Small codebase should have no hub utility noise. Got: {hub_patterns:?}"
    );
}

#[test]
fn rich_fixture_no_hub_noise() {
    // The rich fixture has 2 modules with ~8 symbols — still too small for hub noise
    let ti = common::build_and_load_index(common::make_rich_test_docs());
    let hub_patterns = noise_patterns_by_category(ti.index(), "Hub utilities");
    assert!(
        hub_patterns.is_empty(),
        "2-module fixture should have no hub utility noise. Got: {hub_patterns:?}"
    );
}
