use std::path::Path;

use crate::error::{AppError, Result};
use crate::models::problem::ProblemBundleSpec;

pub async fn read_problem_bundle_spec(bundle_dir: &Path) -> Result<ProblemBundleSpec> {
    let spec_path = bundle_dir.join("spec.json");
    let raw = tokio::fs::read_to_string(&spec_path).await?;
    let spec: ProblemBundleSpec = serde_json::from_str(&raw)
        .map_err(|e| AppError::Validation(format!("invalid spec.json: {e}")))?;
    Ok(spec)
}

pub async fn validate_problem_bundle(bundle_dir: &Path) -> Result<()> {
    let spec = read_problem_bundle_spec(bundle_dir).await?;
    let spec_path = bundle_dir.join("spec.json");
    let statement_path = bundle_dir.join("statement.md");
    let scorer_path = bundle_dir.join(&spec.scorer.entrypoint);
    let shown_dir = bundle_dir.join(&spec.datasets.shown_dir);
    let hidden_dir = bundle_dir.join(&spec.datasets.hidden_dir);

    assert_path_type(&spec_path, "file", "spec.json").await?;
    assert_path_type(&statement_path, "file", "statement.md").await?;
    assert_path_type(&scorer_path, "file", "scorer entrypoint").await?;
    assert_path_type(&shown_dir, "directory", "shown data dir").await?;
    assert_path_type(&hidden_dir, "directory", "hidden data dir").await?;

    if let Some(ref heldout_dir) = spec.datasets.heldout_dir {
        assert_path_type(&bundle_dir.join(heldout_dir), "directory", "heldout data dir").await?;
    }

    Ok(())
}

async fn assert_path_type(path: &Path, kind: &str, label: &str) -> Result<()> {
    let meta = tokio::fs::metadata(path).await.map_err(|_| {
        AppError::Validation(format!("{} does not exist: {}", label, path.display()))
    })?;

    if kind == "file" && !meta.is_file() {
        return Err(AppError::Validation(format!("{} is not a file: {}", label, path.display())));
    }
    if kind == "directory" && !meta.is_dir() {
        return Err(AppError::Validation(format!("{} is not a directory: {}", label, path.display())));
    }

    Ok(())
}

pub async fn extract_problem_description(statement_path: &Path) -> Result<String> {
    let content = tokio::fs::read_to_string(statement_path).await?;
    let lines: Vec<&str> = content.lines().collect();
    let mut paragraph: Vec<String> = Vec::new();
    let mut in_code_block = false;

    for raw_line in lines {
        let line = raw_line.trim();

        if line.starts_with("```") {
            in_code_block = !in_code_block;
            continue;
        }

        if in_code_block {
            continue;
        }

        if line.is_empty() {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        if line.starts_with('#')
            || line.starts_with('-')
            || line.starts_with("* ")
            || line.starts_with('>')
            || line.starts_with('|')
            || line.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) && line.contains(". ")
        {
            if !paragraph.is_empty() {
                break;
            }
            continue;
        }

        paragraph.push(strip_markdown_inline(line));
    }

    Ok(paragraph.join(" ").trim().to_string())
}

fn strip_markdown_inline(value: &str) -> String {
    let mut result = value.to_string();
    // Strip inline code
    while let Some(start) = result.find('`') {
        if let Some(end) = result[start + 1..].find('`') {
            let inner = result[start + 1..start + 1 + end].to_string();
            result.replace_range(start..start + 1 + end + 1, &inner);
        } else {
            break;
        }
    }
    // Strip links
    result = regex_replace(&result, r"\[([^\]]+)\]\([^)]+\)", "$1");
    // Strip bold
    result = regex_replace(&result, r"\*\*([^*]+)\*\*", "$1");
    // Strip italic
    result = regex_replace(&result, r"\*([^*]+)\*", "$1");
    result = regex_replace(&result, r"_([^_]+)_", "$1");
    result.trim().to_string()
}

fn regex_replace(input: &str, pattern: &str, replacement: &str) -> String {
    use regex::Regex;
    Regex::new(pattern).unwrap().replace_all(input, replacement).to_string()
}

pub fn is_safe_relative_path(value: &str) -> bool {
    if value.starts_with('/') {
        return false;
    }
    let segments: Vec<&str> = value.split(['/', '\\']).collect();
    segments.iter().all(|s| !s.is_empty() && *s != "..")
}
