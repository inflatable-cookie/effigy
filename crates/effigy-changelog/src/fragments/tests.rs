use super::*;
use crate::test_support::Scratch;

fn changelog_beside(scratch: &Scratch) -> PathBuf {
    scratch.write("CHANGELOG.md", "# Changelog\n\n## [Unreleased]\n")
}

fn file_names(set: &FragmentSet) -> Vec<&str> {
    set.fragments.iter().map(|f| f.file_name.as_str()).collect()
}

#[test]
fn a_repository_without_the_directory_is_not_adopted() {
    let scratch = Scratch::new("plain");
    let changelog = changelog_beside(&scratch);
    let set = load(&changelog);
    assert!(!set.adopted);
    assert!(set.fragments.is_empty());
    assert!(set.problems.is_empty());
}

#[test]
fn all_five_types_load_and_order_by_category_then_name() {
    let scratch = Scratch::new("types");
    let changelog = changelog_beside(&scratch);
    scratch.write("changelog.d/.gitkeep", "");
    scratch.write("changelog.d/900-b-security.security.md", "Security b.\n");
    scratch.write("changelog.d/900-a-security.security.md", "Security a.\n");
    scratch.write("changelog.d/800-removed.removed.md", "Removed one.\n");
    scratch.write("changelog.d/801-fixed.fixed.md", "Fixed one.\n");
    scratch.write("changelog.d/802-changed.changed.md", "Changed one.\n");
    scratch.write("changelog.d/803-added.added.md", "Added one.\n");

    let set = load(&changelog);
    assert!(set.adopted);
    assert!(set.problems.is_empty(), "{:?}", set.problems);
    assert_eq!(
        file_names(&set),
        vec![
            "803-added.added.md",
            "802-changed.changed.md",
            "801-fixed.fixed.md",
            "800-removed.removed.md",
            "900-a-security.security.md",
            "900-b-security.security.md",
        ]
    );
    let kinds: Vec<CategoryKind> = set.fragments.iter().map(|f| f.kind).collect();
    assert_eq!(
        kinds,
        vec![
            CategoryKind::Added,
            CategoryKind::Changed,
            CategoryKind::Fixed,
            CategoryKind::Removed,
            CategoryKind::Security,
            CategoryKind::Security,
        ]
    );
}

#[test]
fn wrapped_lines_join_into_one_entry_and_contents_are_kept() {
    let scratch = Scratch::new("wrapped");
    let changelog = changelog_beside(&scratch);
    let body = "- Wrapped\n  entry   text\n  continues here.\n";
    scratch.write("changelog.d/804-wrapped.fixed.md", body);

    let set = load(&changelog);
    assert!(set.problems.is_empty(), "{:?}", set.problems);
    assert_eq!(
        set.fragments[0].entry,
        "- Wrapped entry text continues here."
    );
    assert_eq!(set.fragments[0].contents, body.as_bytes());
}

#[test]
fn each_malformed_fragment_is_a_named_diagnostic_and_is_left_out() {
    let cases: [(&str, &str, &str); 6] = [
        (
            "804-unknown.deprecated.md",
            "x\n",
            "unknown type `deprecated`",
        ),
        ("README.md", "x\n", "is missing its type"),
        ("804-empty.fixed.md", "   \n\n", "is empty"),
        (
            "804-two.fixed.md",
            "First paragraph.\n\nSecond paragraph.\n",
            "more than one paragraph",
        ),
        (".804-hidden.fixed.md", "x\n", "invalid name"),
        ("804 spaced.fixed.md", "x\n", "invalid name"),
    ];
    for (file, body, reason) in cases {
        let scratch = Scratch::new("malformed");
        let changelog = changelog_beside(&scratch);
        scratch.write(&format!("changelog.d/{file}"), body);
        let set = load(&changelog);
        assert!(set.fragments.is_empty(), "{file} must not fold");
        assert_eq!(set.problems.len(), 1, "{file}: {:?}", set.problems);
        let problem = &set.problems[0];
        assert_eq!(problem.rule, "fragment-malformed", "{file}");
        assert!(
            problem.message.contains(file),
            "the diagnostic must name the file: {}",
            problem.message
        );
        assert!(
            problem.message.contains(reason),
            "{file}: {}",
            problem.message
        );
        assert!(problem.suggestion.is_some(), "{file} needs a remedy");
    }
}

#[test]
fn a_non_utf8_fragment_is_refused() {
    let scratch = Scratch::new("utf8");
    let changelog = changelog_beside(&scratch);
    let path = scratch.path().join("changelog.d/804-bytes.fixed.md");
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, [0xff, 0xfe, b'x']).unwrap();
    let set = load(&changelog);
    assert!(set.fragments.is_empty());
    assert!(set.problems[0].message.contains("not valid UTF-8"));
}

#[test]
fn names_that_collide_case_insensitively_are_ambiguous_and_both_refused() {
    // Deterministic on every filesystem: the ambiguity refusal names both
    // spellings and carries the refusal rule, without needing the pair to
    // coexist on disk.
    let diagnostic = fragment_problem_ambiguous("804-Slug.fixed.md", "804-slug.fixed.md");
    assert_eq!(diagnostic.rule, "fragment-ambiguous");
    assert!(
        diagnostic.message.contains("804-Slug.fixed.md"),
        "{}",
        diagnostic.message
    );
    assert!(
        diagnostic.message.contains("804-slug.fixed.md"),
        "{}",
        diagnostic.message
    );
    assert!(diagnostic.suggestion.is_some());

    let scratch = Scratch::new("ambiguous");
    let changelog = changelog_beside(&scratch);
    scratch.write("changelog.d/804-Slug.fixed.md", "One.\n");
    scratch.write("changelog.d/804-slug.fixed.md", "Two.\n");
    if std::fs::read_dir(scratch.path().join("changelog.d"))
        .unwrap()
        .count()
        < 2
    {
        // A case-insensitive filesystem stores one file for both names, so no
        // ambiguous pair can exist there; the refusal is for case-sensitive ones.
        return;
    }
    let set = load(&changelog);
    assert!(set.fragments.is_empty());
    assert_eq!(set.problems.len(), 2, "{:?}", set.problems);
    assert!(set.problems.iter().all(|p| p.rule == "fragment-ambiguous"));
}

#[test]
fn a_symlinked_fragment_is_a_path_refusal_and_never_read() {
    let scratch = Scratch::new("symlink");
    let changelog = changelog_beside(&scratch);
    let outside = scratch.write("outside.txt", "Outside text.\n");
    std::fs::create_dir_all(scratch.path().join("changelog.d")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(
        &outside,
        scratch.path().join("changelog.d/804-link.fixed.md"),
    )
    .unwrap();
    let set = load(&changelog);
    assert!(set.fragments.is_empty());
    #[cfg(unix)]
    {
        assert_eq!(set.problems.len(), 1, "{:?}", set.problems);
        assert_eq!(set.problems[0].rule, "fragment-path");
        assert!(set.problems[0].message.contains("is not a regular file"));
    }
}

#[test]
fn a_symlinked_fragment_directory_is_refused_as_a_path() {
    let scratch = Scratch::new("dirlink");
    let changelog = changelog_beside(&scratch);
    let elsewhere = scratch.path().join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(&elsewhere, scratch.path().join("changelog.d")).unwrap();
        let set = load(&changelog);
        assert!(set.fragments.is_empty());
        assert_eq!(set.problems[0].rule, "fragment-path");
    }
}
