//! Adversarial & Stress Challenge Suite for Milestone 3 (search_vault & Vault Sync)
//!
//! Empirically challenges and stress-tests:
//! 1. Directory Pruning: Mixed casing and nested paths (.Git, .OBSIDIAN, Target, Node_Modules, archive, Archive, .trash).
//! 2. Result Capping: Strict capping at 20 matches when >30 files match.
//! 3. UTF-8 Boundary Safety: Slicing across multi-byte Vietnamese Unicode boundaries at byte 120 without panic.
//! 4. File Size & Extension Guards: Bounding to <= 2MB and only .md/.txt files.
//! 5. Edge cases: Empty queries, whitespace queries, non-matching queries, and Unicode case folding.

use liva_native_core::mcp::protocol::{CallToolRequest, ToolContent};
use liva_native_core::mcp::server::NativeMcpServer;
use std::fs;
use std::path::PathBuf;

fn create_temp_vault(prefix: &str) -> PathBuf {
    let nano = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let p = std::env::temp_dir().join(format!(
        "liva_challenger_m3_{prefix}_{}_{nano}",
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&p);
    fs::create_dir_all(&p).expect("failed to create temp test vault");
    p
}

#[tokio::test]
async fn test_adversarial_directory_pruning_nested_mixed_case() {
    let vault_dir = create_temp_vault("pruning_stress");

    // 1. Create valid files that MUST be matched
    fs::write(
        vault_dir.join("root_valid.md"),
        "Top level valid note with ADVERSARIAL_KEYWORD_PRUNE",
    )
    .expect("write root_valid.md");

    fs::create_dir_all(vault_dir.join("allowed_dir/sub_allowed")).expect("create allowed subdirs");
    fs::write(
        vault_dir.join("allowed_dir/sub_allowed/deep_valid.md"),
        "Deep valid note with ADVERSARIAL_KEYWORD_PRUNE",
    )
    .expect("write deep_valid.md");

    // 2. Create nested directories with mixed casing that MUST be pruned
    let excluded_cases = [
        "nested/sub/.Git",
        "deep/path/.OBSIDIAN",
        "level1/level2/Target",
        "vendor/package/Node_Modules",
        "history/old/archive",
        "docs/history/Archive",
        "reports/all/ARCHIVE",
        "hidden/.trash",
    ];

    for rel_path in &excluded_cases {
        let full_dir = vault_dir.join(rel_path);
        fs::create_dir_all(&full_dir).expect("create excluded dir");
        fs::write(
            full_dir.join("leak_check.md"),
            format!(
                "EXCLUDED LEAK CONTENT: file in {} containing ADVERSARIAL_KEYWORD_PRUNE",
                rel_path
            ),
        )
        .expect("write excluded file");
    }

    let server = NativeMcpServer::new(vault_dir.to_str().unwrap());
    let res = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "adversarial_keyword_prune" }),
        })
        .await
        .expect("search_vault call succeeded");

    let _ = fs::remove_dir_all(&vault_dir);

    assert!(!res.is_error, "Tool call should succeed");
    if let ToolContent::Text { text } = &res.content[0] {
        // Must contain allowed files
        assert!(text.contains("root_valid.md"), "Must contain root_valid.md");
        assert!(
            text.contains("allowed_dir/sub_allowed/deep_valid.md"),
            "Must contain deep_valid.md"
        );

        // Must NOT contain any pruned directories or files inside them
        assert!(!text.to_lowercase().contains(".git"), "Must prune .Git");
        assert!(
            !text.to_lowercase().contains(".obsidian"),
            "Must prune .OBSIDIAN"
        );
        assert!(!text.to_lowercase().contains("target"), "Must prune Target");
        assert!(
            !text.to_lowercase().contains("node_modules"),
            "Must prune Node_Modules"
        );
        assert!(
            !text.to_lowercase().contains("archive"),
            "Must prune archive/Archive/ARCHIVE"
        );
        assert!(!text.to_lowercase().contains(".trash"), "Must prune .trash");
        assert!(
            !text.contains("EXCLUDED LEAK CONTENT"),
            "Must not leak excluded content"
        );

        // Exactly 2 valid files found
        assert!(
            text.contains("(2 found)"),
            "Expected exactly 2 found, got: {text}"
        );
    } else {
        panic!("Expected Text content from search_vault");
    }
}

#[tokio::test]
async fn test_adversarial_result_capping_with_35_files() {
    let vault_dir = create_temp_vault("capping_35_files");

    // Populate 35 valid matching files (> 30 files as required)
    for i in 1..=35 {
        fs::write(
            vault_dir.join(format!("document_{:02}.md", i)),
            format!(
                "Line content for document {:02} with TARGET_CAPPING_TOKEN in body.",
                i
            ),
        )
        .expect("write doc");
    }

    let server = NativeMcpServer::new(vault_dir.to_str().unwrap());
    let res = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "target_capping_token" }),
        })
        .await
        .expect("search_vault call succeeded");

    let _ = fs::remove_dir_all(&vault_dir);

    assert!(!res.is_error, "Tool call should succeed");
    if let ToolContent::Text { text } = &res.content[0] {
        assert!(
            text.contains("(20 found)"),
            "Header must indicate strictly 20 found, got: {text}"
        );
        let items_listed = text.lines().filter(|l| l.starts_with("- ")).count();
        assert_eq!(
            items_listed, 20,
            "Must be strictly capped at 20 items despite 35 files existing"
        );
    } else {
        panic!("Expected Text content");
    }
}

#[tokio::test]
async fn test_adversarial_utf8_vietnamese_boundary_and_accents() {
    let vault_dir = create_temp_vault("utf8_vietnamese_stress");

    // Case A: 119 ASCII bytes + 3-byte Vietnamese 'ế' (indices 119, 120, 121)
    // Slicing at 120 without boundary check would crash inside 'ế'.
    let prefix_119 = "A".repeat(119);
    let line_split_at_120 = format!(
        "{}ế_VIỆT_NAM_HÀ_NỘI_ĐÀ_NẴNG_SÀI_GÒN_KIẾN_TRÚC_HỆ_THỐNG",
        prefix_119
    );
    assert_eq!(prefix_119.len(), 119);
    assert!(
        !line_split_at_120.is_char_boundary(120),
        "Byte 120 must be mid-character"
    );

    fs::write(
        vault_dir.join("vietnamese_boundary_split.md"),
        format!("Header note\n{}\nFooter note", line_split_at_120),
    )
    .expect("write boundary split file");

    // Case B: 118 ASCII bytes + 3-byte Vietnamese 'ộ' (indices 118, 119, 120)
    let prefix_118 = "B".repeat(118);
    let line_split_at_120_b = format!("{}ộ_TIẾNG_VIỆT_CÓ_DẤU_ĐẶC_BIỆT_KHÔNG_PANIC", prefix_118);
    assert_eq!(prefix_118.len(), 118);
    assert!(
        !line_split_at_120_b.is_char_boundary(120),
        "Byte 120 must be mid-character"
    );

    fs::write(
        vault_dir.join("vietnamese_boundary_split_b.md"),
        format!("Header note\n{}\nFooter note", line_split_at_120_b),
    )
    .expect("write boundary split b file");

    // Case C: Vietnamese search query with Unicode case folding
    fs::write(
        vault_dir.join("vietnamese_case_folding.md"),
        "Kiến trúc trợ lý ảo LIVA hỗ trợ xử lý ngôn ngữ Tiếng Việt độ trễ cực thấp.",
    )
    .expect("write case folding note");

    let server = NativeMcpServer::new(vault_dir.to_str().unwrap());

    // Query 1: Triggers snippet truncation on line_split_at_120
    let res1 = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "HÀ_NỘI" }),
        })
        .await
        .expect("search_vault call succeeded");

    assert!(!res1.is_error);
    if let ToolContent::Text { text } = &res1.content[0] {
        assert!(text.contains("vietnamese_boundary_split.md"));
        assert!(text.contains("Preview:"), "Must produce preview snippet");
        assert!(text.contains("..."), "Must include truncation ellipsis");
    }

    // Query 2: Triggers snippet truncation on line_split_at_120_b
    let res2 = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "ĐẶC_BIỆT" }),
        })
        .await
        .expect("search_vault call succeeded");

    assert!(!res2.is_error);
    if let ToolContent::Text { text } = &res2.content[0] {
        assert!(text.contains("vietnamese_boundary_split_b.md"));
        assert!(text.contains("Preview:"));
    }

    // Query 3: Lowercase Vietnamese search query matching uppercase in content
    let res3 = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "tiếng việt" }),
        })
        .await
        .expect("search_vault call succeeded");

    assert!(!res3.is_error);
    if let ToolContent::Text { text } = &res3.content[0] {
        assert!(
            text.contains("vietnamese_case_folding.md"),
            "Should match Tiếng Việt via case folding"
        );
    }

    let _ = fs::remove_dir_all(&vault_dir);
}

#[tokio::test]
async fn test_adversarial_file_size_and_extension_filter() {
    let vault_dir = create_temp_vault("size_and_ext_filter");

    // 1. File > 2MB with keyword must be skipped
    let large_file_path = vault_dir.join("oversized.md");
    let large_data = vec![b'x'; 2 * 1024 * 1024 + 1024]; // 2MB + 1KB
    fs::write(&large_file_path, &large_data).expect("write oversized file");

    // 2. Non-markdown / non-txt files must be skipped
    fs::write(
        vault_dir.join("script.py"),
        "print('GUARD_FILTER_TEST_TOKEN')",
    )
    .expect("write script.py");
    fs::write(
        vault_dir.join("data.json"),
        "{\"key\": \"GUARD_FILTER_TEST_TOKEN\"}",
    )
    .expect("write data.json");

    // 3. Valid .txt and .md files must be matched
    fs::write(
        vault_dir.join("valid_text.txt"),
        "A valid plain text note containing GUARD_FILTER_TEST_TOKEN",
    )
    .expect("write valid_text.txt");
    fs::write(
        vault_dir.join("valid_doc.md"),
        "A valid markdown note containing GUARD_FILTER_TEST_TOKEN",
    )
    .expect("write valid_doc.md");

    let server = NativeMcpServer::new(vault_dir.to_str().unwrap());
    let res = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "guard_filter_test_token" }),
        })
        .await
        .expect("search_vault call succeeded");

    let _ = fs::remove_dir_all(&vault_dir);

    assert!(!res.is_error);
    if let ToolContent::Text { text } = &res.content[0] {
        assert!(text.contains("valid_text.txt"), "Must match .txt files");
        assert!(text.contains("valid_doc.md"), "Must match .md files");
        assert!(!text.contains("script.py"), "Must skip .py files");
        assert!(!text.contains("data.json"), "Must skip .json files");
        assert!(!text.contains("oversized.md"), "Must skip files > 2MB");
        assert!(
            text.contains("(2 found)"),
            "Expected exactly 2 valid matches"
        );
    } else {
        panic!("Expected Text content");
    }
}

#[tokio::test]
async fn test_adversarial_empty_whitespace_and_no_match() {
    let vault_dir = create_temp_vault("empty_and_no_match");
    fs::write(vault_dir.join("note.md"), "Sample content").expect("write note");

    let server = NativeMcpServer::new(vault_dir.to_str().unwrap());

    // Empty query
    let res_empty = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "" }),
        })
        .await
        .expect("empty query succeeded");
    if let ToolContent::Text { text } = &res_empty.content[0] {
        assert!(text.contains("No matching files found"));
    }

    // Whitespace query
    let res_ws = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "   \t\n  " }),
        })
        .await
        .expect("ws query succeeded");
    if let ToolContent::Text { text } = &res_ws.content[0] {
        assert!(text.contains("No matching files found"));
    }

    // Non-matching query
    let res_none = server
        .call_tool(CallToolRequest {
            name: "search_vault".to_string(),
            arguments: serde_json::json!({ "query": "NON_EXISTENT_STRING_XYZ_999" }),
        })
        .await
        .expect("non matching query succeeded");
    if let ToolContent::Text { text } = &res_none.content[0] {
        assert!(text.contains("No matching files found"));
    }

    let _ = fs::remove_dir_all(&vault_dir);
}
