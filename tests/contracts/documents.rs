use crate::support::{Project, list, subset};
use serde_json::{Value, json};
use std::fs;
use std::os::unix::fs::symlink;
use std::process::Command;

#[test]
fn empty_project_initialization_creates_the_complete_foundation() {
    let p = Project::new();
    let result = p.ok(&["init"]);
    subset(
        &result,
        &json!({"command":"init","modelCalls":0,"updated":[]}),
    );
    for file in [
        "AGENTS.md",
        "CLAUDE.md",
        "hivex.json",
        ".gitignore",
        "docs/README.md",
        "docs/PRD.md",
        "docs/CONTEXT.md",
        "docs/adr/README.md",
        "docs/guidelines/engineering.md",
        "docs/guidelines/triage-labels.md",
        "docs/procedures/issue-tracker.md",
    ] {
        assert!(list(&result, "created").contains(&json!(file)));
        assert!(!fs::read(p.path(file)).unwrap().is_empty());
    }
    assert_eq!(
        fs::read_to_string(p.path("CLAUDE.md")).unwrap(),
        "@AGENTS.md\n"
    );
    assert!(p.read_json("hivex.json").get("archive").is_some());
    assert!(p.read_json("hivex.json").get("history").is_none());
    for skill in [
        "hivex",
        "hivex-design",
        "hivex-document",
        "hivex-implement",
        "hivex-review",
        "hivex-git",
    ] {
        for family in [".agents", ".claude"] {
            let relative = format!("{family}/skills/{skill}");
            assert!(list(&result, "created").contains(&json!(relative)));
            assert!(!fs::read_link(p.path(&relative)).unwrap().is_absolute());
            assert!(p.path(&format!("{relative}/SKILL.md")).is_file());
        }
        assert_eq!(
            fs::canonicalize(p.path(&format!(".agents/skills/{skill}"))).unwrap(),
            fs::canonicalize(p.path(&format!(".claude/skills/{skill}"))).unwrap()
        );
    }
    assert!(!p.path(".hivex/knowledge.sqlite").exists());
}

#[test]
fn cjk_source_order_and_page_boundaries_remain_compatible() {
    let p = Project::new();
    for (name, text) in [
        ("中.md", "# First common ideograph\n"),
        ("文.md", "# Second common ideograph\n"),
        ("𠀀.md", "# Supplementary ideograph\n"),
    ] {
        p.write(name, text);
    }
    assert_eq!(paths(&p.ok(&["sources"])), ["𠀀.md", "中.md", "文.md"]);
    assert_eq!(paths(&p.ok(&["sources", "--limit", "1"])), ["𠀀.md"]);
}

fn paths(value: &Value) -> Vec<&str> {
    list(value, "documents")
        .iter()
        .map(|doc| doc["path"].as_str().unwrap())
        .collect()
}
fn document<'a>(value: &'a Value, path: &str) -> &'a Value {
    list(value, "documents")
        .iter()
        .find(|doc| doc["path"] == path)
        .unwrap()
}

#[test]
fn help_errors_and_bounded_diagnostics() {
    let p = Project::new();
    p.write("a.md", "# One\n");
    assert_eq!(p.ok(&[])["application"], "hivex");
    assert_eq!(p.ok(&["--help"])["application"], "hivex");
    for (args, code) in [
        (vec!["unknown"], "INVALID_ARGUMENT"),
        (vec!["--unknown"], "READ_FAILED"),
        (vec!["-x"], "READ_FAILED"),
        (vec!["-abc"], "READ_FAILED"),
        (vec!["unknown", "--unknown"], "READ_FAILED"),
        (vec!["sources", "--unknown"], "INVALID_ARGUMENT"),
        (vec!["sources", "--root"], "INVALID_ARGUMENT"),
        (vec!["sources", "--root", "--unknown"], "INVALID_ARGUMENT"),
        (vec!["sources", "extra"], "INVALID_ARGUMENT"),
        (vec!["read"], "INVALID_ARGUMENT"),
        (vec!["read", "missing.md"], "SOURCE_NOT_FOUND"),
        (vec!["read", "a.md", "--from", "0"], "INVALID_ARGUMENT"),
        (vec!["read", "a.md", "--max-bytes", "1"], "OUTPUT_LIMIT"),
        (vec!["prune", "--keep-caches", "-1"], "READ_FAILED"),
        (vec!["recover", "--keep-caches", "1"], "INVALID_ARGUMENT"),
        (vec!["prune", "--acknowledge-uncertain"], "INVALID_ARGUMENT"),
        (vec!["init", "extra"], "INVALID_ARGUMENT"),
    ] {
        assert_eq!(p.error(&args)["error"]["code"], code, "{args:?}");
    }
    let error = p.error(&["read", &"😀".repeat(200)]);
    subset(
        &error,
        &json!({"error":{"code":"SOURCE_NOT_FOUND","details":{"omitted":true},"messageTruncated":true}}),
    );
    assert_eq!(error["error"]["message"].as_str().unwrap().len(), 254);
}

#[test]
fn discovery_exact_hashes_links_and_custom_layout() {
    let p = Project::new();
    p.write("hivex.json", "\u{feff}{\"include\":[\"**/*.md\"]}");
    let text = "---\ntitle: Package choice\nstatus: draft\n---\n\nSee [module](../module.md).\r\n";
    p.write("packages/core/decision.md", text);
    p.write("packages/module.md", "# Module\n");
    p.write("packages/vendor/ignored.md", "# Vendor\n");
    p.write(".private/ignored.md", "# Private\n");
    let result = p.ok(&["sources"]);
    assert_eq!(
        paths(&result),
        ["packages/core/decision.md", "packages/module.md"]
    );
    subset(&result, &json!({"origin":"current-worktree","warnings":[]}));
    subset(
        document(&result, "packages/core/decision.md"),
        &json!({"hash":"c2e968f15088b7999576c97d666cd94d938b6160777a67a14036763aa03b9e30","title":"Package choice","status":"draft","links":["packages/module.md"]}),
    );
    assert!(
        document(&result, "packages/core/decision.md")
            .get("text")
            .is_none()
    );
    let read = p.ok(&["read", "packages/core/decision.md"]);
    assert_eq!(read["text"], text);
    assert!(read["source"].get("text").is_none());
    assert!(read["continuation"].is_null());
    p.write("packages/module.md", "# Changed\n");
    assert_ne!(result["snapshot"], p.ok(&["sources"])["snapshot"]);
    p.json("hivex.json", &json!({"include":["packages/core/**/*.md"]}));
    assert_eq!(paths(&p.ok(&["sources"])), ["packages/core/decision.md"]);
    p.json(
        "hivex.json",
        &json!({"version":1,"collections":[{"id":"legacy","include":["docs/**/*.md"]}]}),
    );
    assert!(
        p.error(&["sources"])["error"]["message"]
            .as_str()
            .unwrap()
            .contains("legacy collections")
    );
    for config in [
        json!({"archive":[],"history":[]}),
        json!({"archive":null}),
        json!({"archive":"docs/**"}),
        json!({"archive":["../outside.md"]}),
    ] {
        p.json("hivex.json", &config);
        assert_eq!(p.error(&["sources"])["error"]["code"], "INVALID_CONFIG");
    }
}

#[test]
fn source_range_boundaries_preserve_separators_and_unicode() {
    let p = Project::new();
    let text = "\u{feff}# Title\rLine 😀\r\nThird é\nLast\r\n";
    p.write("mixed.md", text);
    p.write("empty.md", "");
    p.write("trailing.md", "a\n\n");
    subset(
        &p.ok(&["read", "mixed.md"]),
        &json!({"lineStart":1,"lineEnd":4,"text":text,"truncated":false}),
    );
    for (args, expected) in [
        (
            vec!["--from", "1", "--to", "1"],
            json!({"text":"\u{feff}# Title","lineEnd":1,"continuation":{"from":2,"to":4,"reason":"range"}}),
        ),
        (
            vec!["--from", "2", "--to", "3"],
            json!({"text":"Line 😀\r\nThird é","lineStart":2,"lineEnd":3,"continuation":{"from":4,"reason":"range"}}),
        ),
        (
            vec!["--max-bytes", "12"],
            json!({"text":"\u{feff}# Title","lineEnd":1,"truncated":true,"continuation":{"from":2,"reason":"max-bytes"}}),
        ),
        (
            vec!["--max-bytes", "24"],
            json!({"text":"\u{feff}# Title\rLine 😀","lineEnd":2,"truncated":true,"continuation":{"from":3,"reason":"max-bytes"}}),
        ),
        (
            vec!["--max-bytes", "36"],
            json!({"text":"\u{feff}# Title\rLine 😀\r\nThird é","lineEnd":3,"truncated":true,"continuation":{"from":4,"reason":"max-bytes"}}),
        ),
    ] {
        let mut command = vec!["read", "mixed.md"];
        command.extend(args);
        subset(&p.ok(&command), &expected);
    }
    subset(
        &p.ok(&["read", "empty.md"]),
        &json!({"text":"","lineStart":1,"lineEnd":1}),
    );
    subset(
        &p.ok(&["read", "trailing.md"]),
        &json!({"text":"a\n\n","lineEnd":2}),
    );
    assert_eq!(
        p.error(&["read", "mixed.md", "--max-bytes", "1"])["error"]["code"],
        "OUTPUT_LIMIT"
    );
    assert!(
        p.error(&["read", "mixed.md", "--from", "4", "--to", "2"])["error"]["message"]
            .as_str()
            .unwrap()
            .contains("outside the source")
    );
}

#[test]
fn glob_matrix_keeps_explicit_hidden_and_vendor_selection() {
    let p = Project::new();
    for name in [
        ".decisions/e.md",
        ".git/secret.md",
        "a.md",
        "docs/b.md",
        "docs/c.markdown",
        "docs/deep/d.md",
        "vendor/f.md",
    ] {
        p.write(name, "# Rule\n");
    }
    for (config, expected) in [
        (
            json!({"include":["**/*.{md,markdown}"]}),
            vec!["a.md", "docs/b.md", "docs/c.markdown", "docs/deep/d.md"],
        ),
        (
            json!({"include":["docs/?.md",".decisions/**","vendor/**"]}),
            vec![".decisions/e.md", "docs/b.md", "vendor/f.md"],
        ),
        (
            json!({"include":["docs/[bc].*"]}),
            vec!["docs/b.md", "docs/c.markdown"],
        ),
        (json!({"include":["!!a.md"]}), vec!["a.md"]),
        (
            json!({"include":["!!!a.md"]}),
            vec!["docs/b.md", "docs/c.markdown", "docs/deep/d.md"],
        ),
        (json!({"exclude":["!!docs/**"]}), vec!["a.md"]),
        (json!({"exclude":["!docs/deep/**"]}), vec!["docs/deep/d.md"]),
        (
            json!({"exclude":["docs/*"]}),
            vec!["a.md", "docs/deep/d.md"],
        ),
        (
            json!({"exclude":["docs/**"],"archive":["docs/deep/**"]}),
            vec!["a.md"],
        ),
        (
            json!({"include":["docs\\**\\*.md"]}),
            vec!["docs/b.md", "docs/deep/d.md"],
        ),
        (json!({"include":[".git/**"]}), vec![]),
    ] {
        p.json("hivex.json", &config);
        assert_eq!(paths(&p.ok(&["sources"])), expected, "{config}");
    }
}

#[test]
fn excluded_subtrees_symlinks_invalid_utf8_and_history_are_bounded() {
    let p = Project::new();
    let outside = Project::new();
    outside.write("secret.md", "# Secret\n");
    p.write("app/docs.md", "# Good\n");
    p.write("app/ios/top.md", "# Generated\n");
    p.write("app/ios/nested/guide.md", "# Nested\n");
    symlink(
        outside.path("secret.md"),
        p.path("app/ios/nested/outside.md"),
    )
    .unwrap();
    p.json(
        "hivex.json",
        &json!({"include":["app/**/*.md"],"exclude":["app/ios/**"]}),
    );
    let result = p.ok(&["sources"]);
    assert_eq!(paths(&result), ["app/docs.md"]);
    assert!(list(&result, "warnings").is_empty());
    for (exclude, expected) in [
        ("app/ios/*", vec!["app/docs.md", "app/ios/nested/guide.md"]),
        ("!app/ios/nested/**", vec!["app/ios/nested/guide.md"]),
    ] {
        p.json(
            "hivex.json",
            &json!({"include":["app/**/*.md"],"exclude":[exclude]}),
        );
        let result = p.ok(&["sources"]);
        assert_eq!(paths(&result), expected);
        assert_eq!(
            result["warnings"],
            json!([{"message":"Skipped symbolic link","path":"app/ios/nested/outside.md"}])
        );
    }
    p.write("app/invalid.md", [195, 40]);
    p.json("hivex.json", &json!({"include":["app/**/*.md"]}));
    assert!(p.ok(&["sources"])["warnings"].to_string().contains("UTF-8"));
    let archive = Project::new();
    for index in 0..2048 {
        archive.write(&format!("archive/{index}.md"), "# Old\n");
    }
    archive.write("current.md", "# Current\n");
    archive.json("hivex.json", &json!({"archive":["archive/**/*.md"]}));
    subset(
        &archive.ok(&["read", "current.md"]),
        &json!({"source":{"historical":false},"text":"# Current\n"}),
    );
    assert_eq!(
        archive.ok(&["read", "archive/0.md"])["source"]["historical"],
        true
    );
    archive.json(
        "hivex.json",
        &json!({"archive":["archive/**"],"exclude":["archive/**"]}),
    );
    assert_eq!(
        archive.error(&["read", "archive/0.md"])["error"]["code"],
        "SOURCE_NOT_FOUND"
    );
}

#[test]
fn initialization_is_repeatable_preserves_owned_bytes_and_git_visibility() {
    let p = Project::new();
    let config = "{\"include\":[\"custom/**/*.md\"]}\r\n";
    p.write("hivex.json", config);
    p.write("docs/CONTEXT.md", "# Owner context\r\n");
    p.write("CLAUDE.md", "# Owner instructions\r\n");
    p.write(".agents/skills/hivex/SKILL.md", "# Owner skill\n");
    fs::create_dir_all(p.path(".claude/skills")).unwrap();
    symlink("missing-owner-skill", p.path(".claude/skills/hivex-review")).unwrap();
    p.write(".hivex/graph.json", "{\"graph\":\"owned\"}\n");
    p.write(
        ".gitignore",
        "!/.hivex/\r\n/.hivex/*\r\n!/.hivex/graph.json\r\n.hivex/\r\n!/.hivex/knowledge.sqlite",
    );
    p.git(&["init", "-q"]);
    subset(
        &p.ok(&["init"]),
        &json!({"command":"init","modelCalls":0,"updated":[".gitignore"]}),
    );
    let ignore = fs::read(p.path(".gitignore")).unwrap();
    subset(
        &p.ok(&["init"]),
        &json!({"created":[],"updated":[],"modelCalls":0}),
    );
    assert_eq!(fs::read(p.path(".gitignore")).unwrap(), ignore);
    assert_eq!(fs::read_to_string(p.path("hivex.json")).unwrap(), config);
    assert_eq!(
        fs::read_to_string(p.path("CLAUDE.md")).unwrap(),
        "# Owner instructions\r\n"
    );
    assert_eq!(
        fs::read_to_string(p.path(".agents/skills/hivex/SKILL.md")).unwrap(),
        "# Owner skill\n"
    );
    assert_eq!(
        fs::read_link(p.path(".claude/skills/hivex-review"))
            .unwrap()
            .to_str(),
        Some("missing-owner-skill")
    );
    assert_eq!(
        fs::read_to_string(p.path("docs/CONTEXT.md")).unwrap(),
        "# Owner context\r\n"
    );
    assert_eq!(
        fs::read_to_string(p.path(".hivex/graph.json")).unwrap(),
        "{\"graph\":\"owned\"}\n"
    );
    for file in [
        "AGENTS.md",
        "docs/README.md",
        "docs/PRD.md",
        "docs/adr/README.md",
        "docs/guidelines/engineering.md",
        "docs/guidelines/triage-labels.md",
        "docs/procedures/issue-tracker.md",
    ] {
        assert!(p.path(file).is_file());
    }
    for (file, code) in [(".hivex/knowledge.sqlite", 0), (".hivex/graph.json", 1)] {
        assert_eq!(
            Command::new("git")
                .current_dir(&p.root)
                .args(["check-ignore", "--no-index", "-q", "--", file])
                .status()
                .unwrap()
                .code(),
            Some(code)
        );
    }
}

#[test]
fn initialization_refuses_symlinks_and_conflicting_nested_ignores_atomically() {
    for relative in [
        "docs",
        ".hivex",
        ".hivex/.gitignore",
        ".agents",
        ".agents/skills",
        ".claude",
        ".claude/skills",
    ] {
        let p = Project::new();
        let outside = Project::new();
        fs::create_dir_all(p.path(relative).parent().unwrap()).unwrap();
        symlink(&outside.root, p.path(relative)).unwrap();
        assert_eq!(p.error(&["init"])["error"]["code"], "INVALID_DESTINATION");
        assert!(!p.path("AGENTS.md").exists());
        assert_eq!(fs::read_dir(&outside.root).unwrap().count(), 0);
    }
    let p = Project::new();
    p.write(".agents/skills/hivex", "occupied by an owner file");
    assert_eq!(p.error(&["init"])["error"]["code"], "INVALID_DESTINATION");
    assert!(!p.path("AGENTS.md").exists());
    assert_eq!(
        fs::read_to_string(p.path(".agents/skills/hivex")).unwrap(),
        "occupied by an owner file"
    );
    let p = Project::new();
    let standalone = p.path("hivex");
    fs::copy(p.command(&[]).get_program(), &standalone).unwrap();
    let output = Command::new(standalone)
        .current_dir(&p.root)
        .arg("init")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert!(output.stdout.is_empty());
    let result: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(result["error"]["code"], "INIT_SKILLS_UNAVAILABLE");
    assert!(!p.path("AGENTS.md").exists());
    for rule in ["!knowledge.sqlite", "*"] {
        let p = Project::new();
        let files = [
            (".gitignore", "/.hivex/\r\n"),
            (".hivex/graph.json", "{\"owned\":\"graph\"}\r\n"),
            ("AGENTS.md", "# Owner\r\n"),
            ("hivex.json", "{\"include\":[\"**/*.md\"]}\r\n"),
        ];
        for (file, text) in files {
            p.write(file, text);
        }
        p.write(".hivex/.gitignore", format!("# Local rules\r\n{rule}\r\n"));
        p.write(".hivex/knowledge.sqlite", [0, 255, 127, 1]);
        assert_eq!(p.error(&["init"])["error"]["code"], "INIT_IGNORE_CONFLICT");
        for (file, text) in files {
            assert_eq!(fs::read_to_string(p.path(file)).unwrap(), text);
        }
        assert_eq!(
            fs::read(p.path(".hivex/knowledge.sqlite")).unwrap(),
            [0, 255, 127, 1]
        );
        assert!(!p.path("docs/PRD.md").exists());
    }
    for text in ["", " \t\r\n", "# Local state\r\n\r\n# No active rules\r\n"] {
        let p = Project::new();
        p.write(".hivex/.gitignore", text);
        p.ok(&["init"]);
        assert_eq!(
            fs::read_to_string(p.path(".hivex/.gitignore")).unwrap(),
            text
        );
        subset(&p.ok(&["init"]), &json!({"created":[],"updated":[]}));
    }
}

#[test]
fn initialization_preserves_all_markdown_extensions_and_marks_history() {
    for extension in ["md", "markdown", "mdown"] {
        let p = Project::new();
        let names = [
            format!("guide.{extension}"),
            format!("docs/archive/old.{extension}"),
            format!("packages/core/docs/archive/old.{extension}"),
        ];
        for name in &names {
            p.write(name, "# Existing source\n\nKeep the original evidence.\n");
            assert_eq!(p.ok(&["read", name])["source"]["historical"], false);
        }
        p.ok(&["init"]);
        for (index, name) in names.iter().enumerate() {
            subset(
                &p.ok(&["read", name]),
                &json!({"text":"# Existing source\n\nKeep the original evidence.\n","source":{"historical":index>0}}),
            );
        }
    }
}

#[test]
fn legacy_source_byte_budgets_have_exact_page_boundaries() {
    let p = Project::new();
    for (file, text) in [
        ("b.md", "# Upper\n"),
        ("c.md", "# A\n"),
        (
            "docs/aliases.md",
            "---\ntitle: &title Wrong\nstatus: *title\n---\n# Alias fallback\n",
        ),
        (
            "docs/anchor.md",
            "---\ntitle: &title Anchored title\nstatus: accepted\n---\n# Fallback\n",
        ),
        ("docs/archive/old.md", "# Old\n"),
        (
            "docs/block.md",
            "---\ntitle: |-\n  *Literal title\nstatus: accepted # *not-an-alias\n---\n# Fallback\n",
        ),
        (
            "docs/duplicates.md",
            "---\ntitle: One\ntitle: Two\n---\n# Fallback\n",
        ),
        (
            "docs/guide.md",
            "---\r\ntitle: \"Guide: decisions\"\r\nstatus: accepted\r\n---\r\n# Heading\r\n[unicode](../%F0%9F%98%80.md#part) [ref][RULE] ![image](../c.md)\r\n[RULE]: ../b.md\r\n",
        ),
        (
            "docs/heading.md",
            "# *Emphasis* and `code` ![alt](img.png) &amp; **strong**\n",
        ),
        (
            "docs/nested-heading.md",
            "> # Quoted heading\n\n# Top heading\n",
        ),
        ("é.md", "# Accent\n"),
        ("𐀀.md", "# Supplementary\n"),
        ("😀.md", "# Emoji\n"),
        ("\u{e000}.md", "# Private Unicode\n"),
    ] {
        p.write(file, text);
    }
    p.json("hivex.json", &json!({"archive":["docs/archive/**"]}));
    let result = p.ok(&["sources"]);
    assert_eq!(
        paths(&result),
        [
            "😀.md",
            "b.md",
            "c.md",
            "docs/aliases.md",
            "docs/anchor.md",
            "docs/archive/old.md",
            "docs/block.md",
            "docs/duplicates.md",
            "docs/guide.md",
            "docs/heading.md",
            "docs/nested-heading.md",
            "é.md",
            "𐀀.md",
            "\u{e000}.md"
        ]
    );
    subset(
        document(&result, "\u{e000}.md"),
        &json!({"hash":"f137c6a83e4fcb357a31725b9d6b6bc835551d1818856a419059f88f3f301396","title":"Private Unicode","historical":false}),
    );
    assert_eq!(
        document(&result, "docs/heading.md")["title"],
        "Emphasis and code alt & strong"
    );
    assert_eq!(
        document(&result, "docs/nested-heading.md")["title"],
        "Top heading"
    );
    subset(
        document(&result, "docs/guide.md"),
        &json!({"title":"Guide: decisions","status":"accepted","links":["😀.md"]}),
    );
    for (bytes, expected) in [
        ("500", vec!["😀.md"]),
        ("1000", vec!["😀.md", "b.md", "c.md", "docs/aliases.md"]),
        (
            "2000",
            vec![
                "😀.md",
                "b.md",
                "c.md",
                "docs/aliases.md",
                "docs/anchor.md",
                "docs/archive/old.md",
                "docs/block.md",
                "docs/duplicates.md",
                "docs/guide.md",
            ],
        ),
    ] {
        assert_eq!(paths(&p.ok(&["sources", "--max-bytes", bytes])), expected);
    }
    let first = p.ok(&["sources", "--limit", "1"]);
    let cursor = first["continuation"].as_str().unwrap();
    assert_eq!(
        paths(&p.ok(&["sources", "--limit", "1", "--cursor", cursor])),
        ["b.md"]
    );
    p.write("c.md", "# Changed\n");
    assert!(
        p.error(&["sources", "--cursor", cursor])["error"]["message"]
            .as_str()
            .unwrap()
            .contains("snapshot")
    );
}
