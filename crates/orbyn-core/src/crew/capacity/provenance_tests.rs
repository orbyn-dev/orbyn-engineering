//! Provenance, the pure parts: diff text to hunks, who wrote each, the note body, and which paths are safe to keep.
//! The git and store parts are in `store/crew_provenance_tests.rs`.

use super::*;

fn hunk(path: &str, start: u32, len: u32) -> Hunk {
    Hunk {
        path: path.to_owned(),
        start,
        len,
    }
}

fn agent() -> Author {
    Author::Agent {
        engine: "claude".into(),
        model: Some("model-1".into()),
    }
}

#[test]
fn a_modified_file_gives_each_changed_range_in_the_new_file() {
    let diff = b"diff --git a/a.txt b/a.txt\nindex 1..2 100644\n--- a/a.txt\n+++ b/a.txt\n\
@@ -2 +2 @@\n-old\n+new\n@@ -5,0 +6,3 @@\n+x\n+y\n+z\n@@ -9,2 +12,0 @@\n-gone\n-gone\n";
    assert_eq!(
        parse_diff(diff),
        vec![hunk("a.txt", 2, 1), hunk("a.txt", 6, 3)],
        "a pure deletion has no new line and no hunk"
    );
}

#[test]
fn an_added_file_starts_at_line_one() {
    let diff = b"diff --git a/n.txt b/n.txt\nnew file mode 100644\nindex 0..1\n--- /dev/null\n+++ b/n.txt\n@@ -0,0 +1,4 @@\n+a\n+b\n+c\n+d\n";
    assert_eq!(parse_diff(diff), vec![hunk("n.txt", 1, 4)]);
}

#[test]
fn a_deleted_file_a_pure_rename_and_a_binary_file_give_nothing() {
    let deleted = b"diff --git a/g.txt b/g.txt\ndeleted file mode 100644\n--- a/g.txt\n+++ /dev/null\n@@ -1,2 +0,0 @@\n-a\n-b\n";
    let renamed =
        b"diff --git a/o.txt b/p.txt\nsimilarity index 100%\nrename from o.txt\nrename to p.txt\n";
    let binary =
        b"diff --git a/b.dat b/b.dat\nindex 1..2 100644\nBinary files a/b.dat and b/b.dat differ\n";
    assert!(parse_diff(deleted).is_empty());
    assert!(parse_diff(renamed).is_empty());
    assert!(parse_diff(binary).is_empty());
}

#[test]
fn a_renamed_and_edited_file_is_kept_under_its_new_name() {
    let diff = b"diff --git a/o.txt b/p.txt\nsimilarity index 90%\nrename from o.txt\nrename to p.txt\nindex 1..2 100644\n--- a/o.txt\n+++ b/p.txt\n@@ -3 +3 @@\n-a\n+b\n";
    assert_eq!(parse_diff(diff), vec![hunk("p.txt", 3, 1)]);
}

#[test]
fn content_that_looks_like_a_header_does_not_start_a_file() {
    // An added line `++ b/evil` shows as `+++ b/evil`; a added line `@ -1 +1 @@` as `+@ -1 +1 @@`.
    let diff = b"diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1,2 +1,2 @@\n-x\n+++ b/evil\n-y\n+@@ -1 +9 @@\n";
    assert_eq!(parse_diff(diff), vec![hunk("a.txt", 1, 2)]);
}

#[test]
fn quoted_and_spaced_paths_are_read_back_and_unsafe_ones_are_dropped() {
    let one = |header: &str| {
        let diff = format!("diff --git x y\n--- a/x\n+++ {header}\n@@ -0,0 +1 @@\n+a\n");
        parse_diff(diff.as_bytes())
    };
    assert_eq!(one("b/dir/a b.txt\t"), vec![hunk("dir/a b.txt", 1, 1)]);
    assert_eq!(one(r#""b/\303\251.txt""#), vec![hunk("\u{e9}.txt", 1, 1)]);
    for bad in [
        "b/../x",
        "b//x",
        "b/./x",
        "b/C:/x",
        r"b/a\b",
        r#""b/a\nb""#,
        r#""b/\377.txt""#,
        "b/",
        "x/y",
    ] {
        assert!(one(bad).is_empty(), "{bad} must not be kept");
    }
}

fn blame_text(rows: &[(&str, u32)]) -> Vec<u8> {
    let mut out = String::new();
    for (id, line) in rows {
        out.push_str(&format!(
            "{id} {line} {line} 1\nauthor someone\nfilename x\n\tcontent {line}\n"
        ));
    }
    out.into_bytes()
}

#[test]
fn blame_gives_each_line_its_commit_and_ignores_content_and_metadata() {
    let (a, b) = ("a".repeat(40), "b".repeat(40));
    let mut text = blame_text(&[(&a, 3), (&b, 4)]);
    // A content line that looks like a header starts with a tab, so it is never read.
    text.extend_from_slice(format!("\t{b} 9 9 1\n").as_bytes());
    let map = parse_blame(&text);
    assert_eq!(map.len(), 2);
    assert_eq!(map[&3], a);
    assert_eq!(map[&4], b);
}

#[test]
fn a_merged_hunk_splits_into_runs_and_only_a_persons_commit_makes_a_person_line() {
    let (agent_id, person_id) = ("a".repeat(40), "b".repeat(40));
    let blame = parse_blame(&blame_text(&[
        (&agent_id, 6),
        (&agent_id, 7),
        (&person_id, 8),
        (&agent_id, 9),
    ]));
    let person: std::collections::HashSet<String> = [person_id].into();
    let out = split_by_author(&hunk("a.txt", 6, 5), &blame, &person, &agent());
    let got: Vec<(u32, u32, bool)> = out
        .iter()
        .map(|h| (h.hunk.start, h.hunk.len, h.author == Author::Person))
        .collect();
    // Line 10 has no blame row: unknown is the agent's.
    assert_eq!(got, [(6, 2, false), (8, 1, true), (9, 2, false)]);
}

#[test]
fn with_no_person_commit_every_line_is_the_agents() {
    let out = split_by_author(
        &hunk("a", 1, 4),
        &Default::default(),
        &Default::default(),
        &agent(),
    );
    assert_eq!(out.len(), 1);
    assert_eq!(out[0].hunk, hunk("a", 1, 4));
}

#[test]
fn the_note_holds_ids_ranges_and_counts_and_nothing_else() {
    let hunks = vec![
        AuthoredHunk {
            hunk: hunk("a.txt", 2, 3),
            author: agent(),
        },
        AuthoredHunk {
            hunk: hunk("a.txt", 9, 1),
            author: Author::Person,
        },
        AuthoredHunk {
            hunk: hunk("b.txt", 1, 1),
            author: agent(),
        },
    ];
    let note = trace_note(&"a".repeat(40), &hunks, 77);
    assert_eq!(note["version"], "0.1.0");
    assert_eq!(note["vcs"]["revision"], "a".repeat(40));
    assert_eq!(
        note["counts"],
        serde_json::json!({ "files": 2, "hunks": 3, "lines": 5 })
    );
    let a = &note["files"][0];
    assert_eq!(a["path"], "a.txt");
    assert_eq!(a["conversations"][0]["contributor"]["type"], "agent");
    assert_eq!(a["conversations"][0]["contributor"]["engine"], "claude");
    assert_eq!(a["conversations"][0]["contributor"]["model"], "model-1");
    assert_eq!(
        a["conversations"][0]["ranges"],
        serde_json::json!([{ "start_line": 2, "end_line": 4 }])
    );
    assert_eq!(
        a["conversations"][1]["contributor"],
        serde_json::json!({ "type": "person" })
    );
    let mut keys: Vec<&String> = note.as_object().unwrap().keys().collect();
    keys.sort();
    assert_eq!(keys, ["counts", "files", "timestamp", "vcs", "version"]);
}

#[test]
fn revisions_must_be_full_hex_ids() {
    assert!(is_revision(&"a".repeat(40)));
    assert!(is_revision(&"0".repeat(64)));
    for bad in ["", "HEAD", "--output=x", &"g".repeat(40), &"a".repeat(39)] {
        assert!(!is_revision(bad), "{bad}");
    }
}
