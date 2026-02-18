use std::env;
use std::path::PathBuf;

// Import the kbase library modules
use kbase::config::{Config, LinkSyntax};
use kbase::note;
use kbase::vault;

fn specs_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("specs")
}

#[test]
fn load_specs_vault() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    // Should have loaded multiple notes
    let notes = store.list_notes(None, false).expect("Failed to list notes");
    assert!(!notes.is_empty(), "Should have loaded some notes");

    // Verify we have the expected spec files
    let note_titles: Vec<&str> = notes.iter().map(|s| s.as_str()).collect();

    // Check for some known spec files
    assert!(
        note_titles.iter().any(|t| t.contains("Metadata as Graph")),
        "Should have metadata-graph spec"
    );
    assert!(
        note_titles.iter().any(|t| t.contains("SPEC-GUIDE")),
        "Should have SPEC-GUIDE"
    );
}

#[test]
fn filter_by_tag() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    // Filter by spec/feature tag - should include descendants
    let feature_notes = store
        .list_notes(Some("spec/feature"), false)
        .expect("Failed to filter by tag");

    // Should have at least the feature specs (metadata-graph, vault-init, etc.)
    assert!(
        !feature_notes.is_empty(),
        "Should have notes with spec/feature tag"
    );

    // All returned notes should be feature specs
    for note in &feature_notes {
        // Note format is "title (path)"
        assert!(
            note.contains("features/") || note.contains("Feature"),
            "Expected feature spec: {}",
            note
        );
    }
}

#[test]
fn filter_by_parent_tag() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    // Filter by parent tag "spec" - should include all specs (spec/feature, spec/contract, etc.)
    let all_specs = store
        .list_notes(Some("spec"), false)
        .expect("Failed to filter by parent tag");

    let feature_specs = store
        .list_notes(Some("spec/feature"), false)
        .expect("Failed to filter by child tag");

    // Parent tag should include at least as many notes as child tag
    assert!(
        all_specs.len() >= feature_specs.len(),
        "Parent tag should include child tag notes: {} vs {}",
        all_specs.len(),
        feature_specs.len()
    );
}

#[test]
fn config_loads_correctly() {
    let vault_path = specs_path();
    let config = Config::load(&vault_path).expect("Failed to load config");

    // specs/.kbase/config.yaml has link_syntax: both
    assert_eq!(
        config.link_syntax,
        LinkSyntax::Both,
        "Expected link_syntax: both"
    );
}

#[test]
fn parse_note_with_frontmatter() {
    let note_path = specs_path().join("features/metadata-graph.md");
    let parsed = note::parse(&note_path, LinkSyntax::Both).expect("Failed to parse note");

    assert_eq!(parsed.title, "Metadata as Graph");
    assert!(parsed.tags.contains(&"spec/feature".to_string()));

    // Check that status field was captured
    assert!(parsed.fields.contains_key("status"));
}

#[test]
fn extract_links_from_spec() {
    let note_path = specs_path().join("features/metadata-graph.md");
    let parsed = note::parse(&note_path, LinkSyntax::Both).expect("Failed to parse note");

    // metadata-graph.md has a markdown link to ../research/validation.md
    assert!(
        parsed.links.iter().any(|l| l.target.contains("validation")),
        "Should have extracted link to validation.md, got: {:?}",
        parsed.links
    );
}

#[test]
fn backlinks_finds_references() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    // metadata-graph links to validation.md, so backlinks for validation should include metadata-graph
    let backlinks = store
        .backlinks("validation")
        .expect("Failed to get backlinks");

    // Should find at least one note linking to validation
    assert!(
        backlinks.iter().any(|b| b.contains("Metadata")),
        "Should find metadata-graph as backlink to validation, got: {:?}",
        backlinks
    );
}

#[test]
fn store_handles_empty_tag_filter() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    // Non-existent tag should return empty
    let notes = store
        .list_notes(Some("nonexistent/tag"), false)
        .expect("Failed to filter by nonexistent tag");

    assert!(notes.is_empty(), "Should have no notes for nonexistent tag");
}

#[test]
fn list_tags_returns_tree() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    let tree = store.list_tags(None, false).expect("Failed to list tags");

    // Should have some tags
    assert!(!tree.is_empty(), "Should have tags");

    // Should have spec as a root tag
    assert!(
        tree.iter().any(|line| line == "spec"),
        "Should have 'spec' root tag, got: {:?}",
        tree
    );

    // Should have child tags with tree formatting
    assert!(
        tree.iter().any(|line| line.contains("feature")),
        "Should have 'feature' child tag, got: {:?}",
        tree
    );
}

#[test]
fn list_tags_with_notes() {
    let vault_path = specs_path();
    let store = vault::load(&vault_path).expect("Failed to load specs vault");

    let tree = store
        .list_tags(Some("spec/feature"), true)
        .expect("Failed to list tags with notes");

    // Should show notes under the tag
    assert!(
        tree.iter().any(|line| line.contains("[")),
        "Should have notes in brackets, got: {:?}",
        tree
    );
}
