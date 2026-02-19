use std::env;
use std::fs;
use std::path::PathBuf;

// Import the kbase library modules
use kbase::config::{Config, LinkSyntax};
use kbase::note;
use kbase::store::Store;
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

// ============================================================================
// Persistent storage tests
// ============================================================================

fn temp_db_path(name: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let unique = COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = env::temp_dir().join(format!("kbase-test-{}-{}-{}", std::process::id(), name, unique));
    fs::create_dir_all(&dir).expect("Failed to create temp dir");
    dir.join("test.db")
}

fn cleanup_temp_db(path: &PathBuf) {
    if let Some(parent) = path.parent() {
        let _ = fs::remove_dir_all(parent);
    }
}

#[test]
fn persistent_store_opens_and_persists() {
    let db_path = temp_db_path("opens");

    // Create store and insert a note
    {
        let store = Store::open(&db_path).expect("Failed to open store");
        let note = note::Note {
            title: "Test Note".to_string(),
            path: PathBuf::from("/tmp/test.md"),
            tags: vec!["test".to_string()],
            tree_edges: vec![],
            fields: Default::default(),
            links: vec![],
        };
        store.upsert_note(&note).expect("Failed to insert note");

        let notes = store.list_notes(None, false).expect("Failed to list");
        assert_eq!(notes.len(), 1);
    }

    // Reopen and verify data persisted
    {
        let store = Store::open(&db_path).expect("Failed to reopen store");
        let notes = store.list_notes(None, false).expect("Failed to list");
        assert_eq!(notes.len(), 1, "Note should persist across reopens");
        assert!(notes[0].contains("Test Note"));
    }

    cleanup_temp_db(&db_path);
}

#[test]
fn persistent_store_tracks_mtime() {
    let db_path = temp_db_path("mtime");

    let store = Store::open(&db_path).expect("Failed to open store");
    let note = note::Note {
        title: "Mtime Test".to_string(),
        path: PathBuf::from("/tmp/mtime-test.md"),
        tags: vec![],
        tree_edges: vec![],
        fields: Default::default(),
        links: vec![],
    };

    // Insert without mtime
    store.upsert_note(&note).expect("Failed to insert");
    let mtime = store
        .get_note_mtime("/tmp/mtime-test.md")
        .expect("Failed to get mtime");
    assert!(mtime.is_none(), "Should have no mtime initially");

    // Insert with mtime
    use std::time::{Duration, SystemTime};
    let test_time = SystemTime::UNIX_EPOCH + Duration::from_secs(1700000000);
    store
        .upsert_note_with_mtime(&note, Some(test_time))
        .expect("Failed to insert with mtime");

    let mtime = store
        .get_note_mtime("/tmp/mtime-test.md")
        .expect("Failed to get mtime");
    assert_eq!(mtime, Some(1700000000), "Should have stored mtime");

    cleanup_temp_db(&db_path);
}

#[test]
fn persistent_store_lists_all_paths() {
    let db_path = temp_db_path("paths");

    let store = Store::open(&db_path).expect("Failed to open store");

    // Insert multiple notes
    for i in 1..=3 {
        let note = note::Note {
            title: format!("Note {}", i),
            path: PathBuf::from(format!("/tmp/note{}.md", i)),
            tags: vec![],
            tree_edges: vec![],
            fields: Default::default(),
            links: vec![],
        };
        store.upsert_note(&note).expect("Failed to insert");
    }

    let paths = store.get_all_note_paths().expect("Failed to get paths");
    assert_eq!(paths.len(), 3);
    assert!(paths.iter().any(|p| p.contains("note1.md")));
    assert!(paths.iter().any(|p| p.contains("note2.md")));
    assert!(paths.iter().any(|p| p.contains("note3.md")));

    cleanup_temp_db(&db_path);
}

#[test]
fn persistent_store_removes_notes() {
    let db_path = temp_db_path("removes");

    let store = Store::open(&db_path).expect("Failed to open store");
    let note = note::Note {
        title: "To Delete".to_string(),
        path: PathBuf::from("/tmp/delete-me.md"),
        tags: vec!["deletable".to_string()],
        tree_edges: vec![],
        fields: Default::default(),
        links: vec![],
    };

    store.upsert_note(&note).expect("Failed to insert");
    assert_eq!(store.list_notes(None, false).unwrap().len(), 1);

    store.remove_note("/tmp/delete-me.md").expect("Failed to remove");
    assert_eq!(store.list_notes(None, false).unwrap().len(), 0);

    cleanup_temp_db(&db_path);
}

#[test]
fn load_persistent_does_incremental_update() {
    // Use a temp copy of specs to avoid locking the real vault
    let temp_vault = env::temp_dir().join(format!("kbase-vault-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_vault); // Clean up any previous run

    // Copy specs to temp location
    fn copy_dir_recursive(src: &PathBuf, dst: &PathBuf) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).unwrap();
            }
        }
    }
    copy_dir_recursive(&specs_path(), &temp_vault);

    // First load - should process all notes
    let notes_count;
    {
        let (store1, updated1) = vault::load_persistent(&temp_vault).expect("Failed to load");
        let notes1 = store1.list_notes(None, false).expect("Failed to list");
        assert!(!notes1.is_empty(), "Should have loaded notes");
        assert!(updated1 > 0, "First load should update notes");
        notes_count = notes1.len();
    } // store1 dropped here, releasing lock

    // Second load - nothing changed, should update 0
    {
        let (store2, updated2) = vault::load_persistent(&temp_vault).expect("Failed to reload");
        let notes2 = store2.list_notes(None, false).expect("Failed to list");
        assert_eq!(notes_count, notes2.len(), "Should have same notes");
        assert_eq!(updated2, 0, "Second load should update 0 notes (nothing changed)");
    }

    // Cleanup
    let _ = fs::remove_dir_all(&temp_vault);
}

// ============================================================================
// Tree structure tests
// ============================================================================

#[test]
fn note_is_tree_node() {
    let db_path = temp_db_path("note-is-tree");
    let store = Store::open(&db_path).expect("Failed to open store");

    // Create a note titled "ai" with tag domain/ai
    // This note IS the "ai" node in the domain tree
    let ai_note = note::Note {
        title: "ai".to_string(),
        path: PathBuf::from("/tmp/ai.md"),
        tags: vec!["domain/ai".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&ai_note).expect("Failed to insert ai note");

    // Create a child note under ai
    let llm_note = note::Note {
        title: "llms".to_string(),
        path: PathBuf::from("/tmp/llms.md"),
        tags: vec!["domain/ai/llms".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai/llms").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&llm_note).expect("Failed to insert llm note");

    // "ai" should be a child of "domain" in the tree
    let domain_children = store.tree_children("domain", "domain").expect("Failed to get children");
    assert!(
        domain_children.contains(&"ai".to_string()),
        "ai should be child of domain, got: {:?}",
        domain_children
    );

    // "llms" should be a child of "ai"
    let ai_children = store.tree_children("domain", "ai").expect("Failed to get ai children");
    assert!(
        ai_children.contains(&"llms".to_string()),
        "llms should be child of ai, got: {:?}",
        ai_children
    );

    // Both notes should be listable
    let notes = store.list_notes(None, false).expect("Failed to list");
    assert_eq!(notes.len(), 2);

    cleanup_temp_db(&db_path);
}

#[test]
fn cleanup_orphan_tags_removes_unused() {
    let db_path = temp_db_path("orphan-cleanup");
    let store = Store::open(&db_path).expect("Failed to open store");

    // Create a note deep in hierarchy: domain/ai/ml/deep-learning
    // This creates intermediate nodes "ai" and "ml" that aren't notes themselves
    let note1 = note::Note {
        title: "deep-learning".to_string(),
        path: PathBuf::from("/tmp/dl.md"),
        tags: vec!["domain/ai/ml/deep-learning".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai/ml/deep-learning").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&note1).expect("Failed to insert");

    // Verify hierarchy exists
    let paths = store.all_tree_paths().expect("Failed to get paths");
    assert!(paths.iter().any(|p| p.contains("ai")), "ai should exist: {:?}", paths);
    assert!(paths.iter().any(|p| p.contains("ml")), "ml should exist: {:?}", paths);

    // Remove the note - this removes the note's triples but leaves orphan hierarchy
    store.remove_note("/tmp/dl.md").expect("Failed to remove");

    // Run garbage collection - should clean up ai, ml hierarchy nodes
    let _removed = store.cleanup_orphan_tags().expect("Failed to cleanup");

    // After cleanup, orphan intermediate nodes should be gone
    let paths_after = store.all_tree_paths().expect("Failed to get paths");
    assert!(
        paths_after.is_empty() || !paths_after.iter().any(|p| p.contains("ml")),
        "orphan hierarchy should be cleaned up: {:?}",
        paths_after
    );

    cleanup_temp_db(&db_path);
}

#[test]
fn cleanup_preserves_shared_ancestors() {
    let db_path = temp_db_path("shared-ancestors");
    let store = Store::open(&db_path).expect("Failed to open store");

    // Two notes share ancestor "ai": domain/ai/x and domain/ai/y
    let note1 = note::Note {
        title: "topic-x".to_string(),
        path: PathBuf::from("/tmp/x.md"),
        tags: vec!["domain/ai/topic-x".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai/topic-x").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&note1).expect("Failed to insert");

    let note2 = note::Note {
        title: "topic-y".to_string(),
        path: PathBuf::from("/tmp/y.md"),
        tags: vec!["domain/ai/topic-y".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai/topic-y").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&note2).expect("Failed to insert");

    // Remove note1
    store.remove_note("/tmp/x.md").expect("Failed to remove");
    store.cleanup_orphan_tags().expect("Failed to cleanup");

    // domain/ai should still exist (used by topic-y)
    let paths = store.all_tree_paths().expect("Failed to get paths");
    assert!(
        paths.iter().any(|p| p == "domain/ai" || p.starts_with("domain/ai/")),
        "domain/ai should be preserved: {:?}",
        paths
    );

    cleanup_temp_db(&db_path);
}

// ============================================================================
// Export tests
// ============================================================================

#[test]
fn export_dot_includes_notes_and_edges() {
    let db_path = temp_db_path("export-dot");
    let store = Store::open(&db_path).expect("Failed to open store");

    // Create notes with links and tree edges
    let note1 = note::Note {
        title: "AI".to_string(),
        path: PathBuf::from("/tmp/ai.md"),
        tags: vec!["domain/ai".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai").unwrap()],
        fields: Default::default(),
        links: vec![note::Link {
            target: "ML".to_string(),
            line: 1,
            start_col: 0,
            end_col: 4,
        }],
    };
    store.upsert_note(&note1).expect("Failed to insert");

    let note2 = note::Note {
        title: "ML".to_string(),
        path: PathBuf::from("/tmp/ml.md"),
        tags: vec!["domain/ai/ml".to_string()],
        tree_edges: vec![note::TreeEdge::parse("domain/ai/ml").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&note2).expect("Failed to insert");

    let dot = store.export_dot().expect("Failed to export DOT");

    // Check DOT structure
    assert!(dot.starts_with("digraph vault {"), "Should be valid DOT");
    assert!(dot.contains("n_AI"), "Should have AI node");
    assert!(dot.contains("n_ML"), "Should have ML node");
    assert!(dot.contains("->"), "Should have edges");
    assert!(dot.contains("domain"), "Should have tree edge labels");

    cleanup_temp_db(&db_path);
}

#[test]
fn export_graphml_valid_xml() {
    let db_path = temp_db_path("export-graphml");
    let store = Store::open(&db_path).expect("Failed to open store");

    let note = note::Note {
        title: "Test & Note".to_string(), // Test XML escaping
        path: PathBuf::from("/tmp/test.md"),
        tags: vec!["type/test".to_string()],
        tree_edges: vec![note::TreeEdge::parse("type/test").unwrap()],
        fields: Default::default(),
        links: vec![],
    };
    store.upsert_note(&note).expect("Failed to insert");

    let xml = store.export_graphml().expect("Failed to export GraphML");

    // Check GraphML structure
    assert!(xml.contains("<?xml"), "Should have XML declaration");
    assert!(xml.contains("<graphml"), "Should have graphml root");
    assert!(xml.contains("<node"), "Should have nodes");
    assert!(xml.contains("Test &amp; Note"), "Should escape XML entities");
    assert!(xml.contains("</graphml>"), "Should close graphml tag");

    cleanup_temp_db(&db_path);
}
