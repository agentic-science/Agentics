use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use agentics_contracts::challenge_bundle::MAX_CHALLENGE_RUNS_PER_EVALUATION;
use anyhow::{Context, anyhow};
use serde_json::{Map, Value, json};

use super::validation::build_zip;
use super::{PRIVATE_RUNS_PATH, PRIVATE_SESSION_PATH};

pub(super) struct GeneratedArtifact {
    pub(super) challenge_name: String,
    pub(super) adapter_label: &'static str,
    pub(super) case_count: usize,
    pub(super) selection_note: Option<String>,
    pub(super) required_paths: Vec<String>,
    pub(super) zip_bytes: Vec<u8>,
}

#[derive(Clone)]
struct CaseFile {
    stem: String,
    input_path: PathBuf,
    answer_path: PathBuf,
}

struct GeneratedOverlay {
    adapter_label: &'static str,
    pub(super) case_count: usize,
    pub(super) selection_note: Option<String>,
    entries: BTreeMap<String, Vec<u8>>,
}

pub(super) fn generate_one(
    agentics_challenges_root: &Path,
    frontier_cs_root: &Path,
    challenge_name: &str,
) -> anyhow::Result<GeneratedArtifact> {
    let challenge_root = agentics_challenges_root
        .join("challenges")
        .join(challenge_name);
    let bundle_root = challenge_root.join("v1");
    if !challenge_root.is_dir() {
        anyhow::bail!(
            "challenge directory does not exist: {}",
            challenge_root.display()
        );
    }
    let manifest = load_json_file(&challenge_root.join("agentics.challenge.json"))?;
    let spec = load_json_file(&bundle_root.join("spec.json"))?;
    let problem_id = problem_id_from_challenge_name(challenge_name)?;
    let problem_root = frontier_cs_root
        .join("algorithmic")
        .join("problems")
        .join(problem_id.to_string());
    let cases = collect_case_files(&problem_root)?;
    let required_paths = required_private_paths(&manifest)?;

    let execution = required_object(&spec, "execution")?;
    let mode = required_str(execution, "mode")?;
    let overlay = match mode {
        "separated_evaluator" => generate_separated_overlay(&bundle_root, problem_id, &cases)?,
        "piped_stdio" => generate_piped_overlay(&bundle_root, problem_id, &cases)?,
        other => anyhow::bail!("unsupported Frontier-CS refresh execution mode `{other}`"),
    };

    for required_path in &required_paths {
        if !overlay.entries.contains_key(required_path) {
            anyhow::bail!(
                "generated overlay is missing required private asset path `{required_path}`"
            );
        }
    }
    let zip_bytes = build_zip(&overlay.entries)?;
    Ok(GeneratedArtifact {
        challenge_name: challenge_name.to_string(),
        adapter_label: overlay.adapter_label,
        case_count: overlay.case_count,
        selection_note: overlay.selection_note,
        required_paths,
        zip_bytes,
    })
}

fn generate_separated_overlay(
    bundle_root: &Path,
    problem_id: u32,
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let public_runs = load_json_file(&bundle_root.join("public").join("runs.json"))?;
    let public_run = required_array(&public_runs, "runs")?
        .first()
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("public runs.json must contain at least one object run"))?;

    match problem_id {
        1 => generate_treasure_packing_runs(cases),
        6 => generate_world_map_runs(cases),
        _ if public_run.contains_key("input_path")
            || public_run.contains_key("answer_path")
            || public_run.contains_key("input_files") =>
        {
            generate_template_separated_runs(public_run, cases)
        }
        _ if public_run.contains_key("stdin_text") => generate_stdin_text_runs(public_run, cases),
        _ => anyhow::bail!("could not classify separated-evaluator adapter"),
    }
}

fn generate_piped_overlay(
    bundle_root: &Path,
    problem_id: u32,
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    match problem_id {
        2 => generate_permutation_reconstruction_session(cases),
        30 => generate_moving_mole_session(bundle_root, cases),
        35 => generate_duplicate_position_runtime_session(),
        57 => generate_signed_rooted_runtime_session(cases),
        59 => generate_limited_shuffle_runtime_session(cases),
        70 => generate_treasure_hunt_runtime_session(cases),
        80 => generate_uniform_cave_session(cases),
        _ => {
            let public_session = load_json_file(&bundle_root.join("public").join("session.json"))?;
            if public_session
                .get("input_files")
                .and_then(Value::as_array)
                .is_some_and(|files| !files.is_empty())
            {
                generate_file_backed_piped_session(problem_id, &public_session, cases)
            } else {
                anyhow::bail!("could not classify piped-stdio adapter")
            }
        }
    }
}

fn generate_template_separated_runs(
    public_run: &Map<String, Value>,
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let selected = select_cases(cases, max_challenge_runs_per_evaluation()?)?;
    let mut entries = BTreeMap::new();
    let mut runs = Vec::with_capacity(selected.cases.len());
    for case in &selected.cases {
        let run_name = format!("official-{}", safe_case_stem(&case.stem));
        let input_private_path = format!("private-benchmark/cases/{run_name}.in");
        let answer_private_path = format!("private-benchmark/answers/{run_name}.ans");
        let mut run = public_run.clone();
        run.insert("run_name".to_string(), Value::String(run_name));
        if run.contains_key("stdin_text") {
            run.insert(
                "stdin_text".to_string(),
                Value::String(read_utf8(&case.input_path)?),
            );
        }
        if run.contains_key("answer_text") {
            run.insert(
                "answer_text".to_string(),
                Value::String(read_utf8(&case.answer_path)?),
            );
        }
        if run.contains_key("input_path") {
            run.insert(
                "input_path".to_string(),
                Value::String(input_private_path.clone()),
            );
            entries.insert(input_private_path.clone(), fs::read(&case.input_path)?);
        }
        if run.contains_key("answer_path") {
            run.insert(
                "answer_path".to_string(),
                Value::String(answer_private_path.clone()),
            );
            entries.insert(answer_private_path.clone(), fs::read(&case.answer_path)?);
        }
        if run.contains_key("expected_output_source_path") {
            run.insert(
                "expected_output_source_path".to_string(),
                Value::String(answer_private_path.clone()),
            );
            entries.insert(answer_private_path.clone(), fs::read(&case.answer_path)?);
        }
        if let Some(input_files) = run.get_mut("input_files").and_then(Value::as_array_mut) {
            if input_files.len() != 1 {
                anyhow::bail!("path-backed separated adapters must declare one input file");
            }
            let input = input_files
                .first_mut()
                .ok_or_else(|| anyhow!("runs[].input_files must contain one object"))?
                .as_object_mut()
                .ok_or_else(|| anyhow!("runs[].input_files[] must be an object"))?;
            input.remove("content");
            input.remove("content_json");
            input.insert(
                "source_path".to_string(),
                Value::String(input_private_path.clone()),
            );
            entries.insert(input_private_path, fs::read(&case.input_path)?);
        }
        runs.push(Value::Object(run));
    }
    entries.insert(
        PRIVATE_RUNS_PATH.to_string(),
        json_bytes(json!({ "runs": runs }))?,
    );
    Ok(GeneratedOverlay {
        adapter_label: "separated-template",
        case_count: selected.cases.len(),
        selection_note: selected.note,
        entries,
    })
}

fn generate_stdin_text_runs(
    public_run: &Map<String, Value>,
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let selected = select_cases(cases, max_challenge_runs_per_evaluation()?)?;
    let runs = selected
        .cases
        .iter()
        .map(|case| {
            let mut run = public_run.clone();
            run.insert(
                "run_name".to_string(),
                Value::String(format!("official-{}", safe_case_stem(&case.stem))),
            );
            run.insert(
                "stdin_text".to_string(),
                Value::String(read_utf8(&case.input_path)?),
            );
            if run.contains_key("answer_text") {
                run.insert(
                    "answer_text".to_string(),
                    Value::String(read_utf8(&case.answer_path)?),
                );
            }
            anyhow::Ok(Value::Object(run))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    let mut entries = BTreeMap::new();
    entries.insert(
        PRIVATE_RUNS_PATH.to_string(),
        json_bytes(json!({ "runs": runs }))?,
    );
    Ok(GeneratedOverlay {
        adapter_label: "separated-stdio-text",
        case_count: selected.cases.len(),
        selection_note: selected.note,
        entries,
    })
}

fn generate_treasure_packing_runs(cases: &[CaseFile]) -> anyhow::Result<GeneratedOverlay> {
    let selected = select_cases(cases, max_challenge_runs_per_evaluation()?)?;
    let mut runs = Vec::with_capacity(selected.cases.len());
    for case in &selected.cases {
        let stdin_json: Value = serde_json::from_str(&read_utf8(&case.input_path)?)
            .with_context(|| format!("{} is not JSON", case.input_path.display()))?;
        let answer_tokens = parse_i64_tokens(&read_utf8(&case.answer_path)?);
        if answer_tokens.len() < 2 {
            anyhow::bail!("treasure-packing answer must contain baseline and best values");
        }
        let baseline_value = answer_tokens
            .first()
            .copied()
            .ok_or_else(|| anyhow!("treasure-packing answer is missing baseline value"))?;
        let best_value = answer_tokens
            .get(1)
            .copied()
            .ok_or_else(|| anyhow!("treasure-packing answer is missing best value"))?;
        runs.push(json!({
            "run_name": format!("official-{}", safe_case_stem(&case.stem)),
            "interface": "stdio",
            "stdin_json": stdin_json,
            "baseline_value": baseline_value,
            "best_value": best_value
        }));
    }
    let mut entries = BTreeMap::new();
    entries.insert(
        PRIVATE_RUNS_PATH.to_string(),
        json_bytes(json!({ "runs": runs }))?,
    );
    Ok(GeneratedOverlay {
        adapter_label: "separated-treasure-packing",
        case_count: selected.cases.len(),
        selection_note: selected.note,
        entries,
    })
}

fn generate_world_map_runs(cases: &[CaseFile]) -> anyhow::Result<GeneratedOverlay> {
    let mut expanded = Vec::new();
    for case in cases {
        for (index, case_text) in split_world_map_cases(&read_utf8(&case.input_path)?)?
            .into_iter()
            .enumerate()
        {
            let case_index = index
                .checked_add(1)
                .ok_or_else(|| anyhow!("world-map expanded case index overflow"))?;
            expanded.push((
                format!("{}-{case_index}", safe_case_stem(&case.stem)),
                case_text,
            ));
        }
    }
    let selected = select_named_values(expanded, max_challenge_runs_per_evaluation()?)?;
    let runs = selected
        .values
        .iter()
        .map(|(name, stdin_text)| {
            json!({
                "run_name": format!("official-{name}"),
                "interface": "stdio",
                "stdin_text": stdin_text
            })
        })
        .collect::<Vec<_>>();
    let mut entries = BTreeMap::new();
    entries.insert(
        PRIVATE_RUNS_PATH.to_string(),
        json_bytes(json!({ "runs": runs }))?,
    );
    Ok(GeneratedOverlay {
        adapter_label: "separated-world-map",
        case_count: selected.values.len(),
        selection_note: selected.note,
        entries,
    })
}

fn generate_file_backed_piped_session(
    problem_id: u32,
    public_session: &Value,
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let mut entries = BTreeMap::new();
    let mut input_files = Vec::new();
    let mut metadata = public_session
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    apply_file_backed_piped_session_metadata_overrides(problem_id, &mut metadata)?;
    let public_case_template = metadata
        .get("cases")
        .and_then(Value::as_array)
        .and_then(|cases| cases.first())
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut metadata_cases = Vec::with_capacity(cases.len());
    for case in cases {
        let case_name = format!("official-{}", safe_case_stem(&case.stem));
        let source_input_path = format!("private-benchmark/cases/{case_name}.in");
        let source_answer_path = format!("private-benchmark/answers/{case_name}.ans");
        let materialized_input_path = format!("cases/{case_name}.in");
        let materialized_answer_path = format!("answers/{case_name}.ans");
        entries.insert(source_input_path.clone(), fs::read(&case.input_path)?);
        entries.insert(source_answer_path.clone(), fs::read(&case.answer_path)?);
        input_files.push(json!({
            "path": materialized_input_path,
            "source_path": source_input_path
        }));
        input_files.push(json!({
            "path": materialized_answer_path,
            "source_path": source_answer_path
        }));
        let mut metadata_case = public_case_template.clone();
        metadata_case.insert("case_name".to_string(), Value::String(case_name));
        metadata_case.insert(
            "input_path".to_string(),
            Value::String(format!("cases/official-{}.in", safe_case_stem(&case.stem))),
        );
        metadata_case.insert(
            "answer_path".to_string(),
            Value::String(format!(
                "answers/official-{}.ans",
                safe_case_stem(&case.stem)
            )),
        );
        metadata_cases.push(Value::Object(metadata_case));
    }
    metadata.insert("cases".to_string(), Value::Array(metadata_cases));
    let session = json!({
        "session_name": "official",
        "input_files": input_files,
        "metadata": metadata
    });
    entries.insert(PRIVATE_SESSION_PATH.to_string(), json_bytes(session)?);
    Ok(GeneratedOverlay {
        adapter_label: "piped-file-backed",
        case_count: cases.len(),
        selection_note: None,
        entries,
    })
}

fn apply_file_backed_piped_session_metadata_overrides(
    problem_id: u32,
    metadata: &mut Map<String, Value>,
) -> anyhow::Result<()> {
    if problem_id == 14 {
        match metadata.get("case_separator_message") {
            Some(Value::String(existing)) if existing == "NEXT" => {}
            Some(_) => anyhow::bail!(
                "hidden-cycle-length case_separator_message must be the string `NEXT`"
            ),
            None => {
                metadata.insert(
                    "case_separator_message".to_string(),
                    Value::String("NEXT".to_string()),
                );
            }
        }
    }
    Ok(())
}

fn generate_permutation_reconstruction_session(
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let selected = select_best_case(cases, |case| {
        parse_i64_tokens(&read_utf8(&case.input_path)?)
            .first()
            .copied()
            .ok_or_else(|| anyhow!("permutation input is empty"))
    })?;
    let n = parse_i64_tokens(&read_utf8(&selected.input_path)?)
        .first()
        .copied()
        .ok_or_else(|| anyhow!("permutation input is empty"))?;
    let answer_tokens = parse_i64_tokens(&read_utf8(&selected.answer_path)?);
    let n_usize = usize::try_from(n).context("permutation n must fit usize")?;
    let permutation_end = n_usize
        .checked_add(1)
        .ok_or_else(|| anyhow!("permutation length overflow"))?;
    if answer_tokens.len() < permutation_end {
        anyhow::bail!("permutation answer is missing best query count or permutation");
    }
    let best_queries = answer_tokens
        .first()
        .copied()
        .ok_or_else(|| anyhow!("permutation answer is missing best query count"))?;
    let permutation = answer_tokens
        .get(1..permutation_end)
        .ok_or_else(|| anyhow!("permutation answer is missing permutation"))?
        .to_vec();
    let session = json!({
        "session_name": "official",
        "metadata": {
            "n": n,
            "permutation": permutation,
            "best_queries": best_queries,
            "max_queries": 10000
        }
    });
    single_session_overlay("piped-permutation-reconstruction", session)
}

fn generate_moving_mole_session(
    bundle_root: &Path,
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let public_session = load_json_file(&bundle_root.join("public").join("session.json"))?;
    let mut entries = BTreeMap::new();
    let mut input_files = Vec::new();
    let mut metadata = public_session
        .get("metadata")
        .and_then(Value::as_object)
        .cloned()
        .unwrap_or_default();
    let mut metadata_cases = Vec::with_capacity(cases.len());
    for case in cases {
        let case_name = format!("official-{}", safe_case_stem(&case.stem));
        let source_input_path = format!("private-benchmark/cases/{case_name}.in");
        let source_answer_path = format!("private-benchmark/answers/{case_name}.ans");
        let materialized_input_path = format!("cases/{case_name}.in");
        let materialized_answer_path = format!("answers/{case_name}.ans");
        entries.insert(source_input_path.clone(), fs::read(&case.input_path)?);
        entries.insert(source_answer_path.clone(), fs::read(&case.answer_path)?);
        input_files.push(json!({
            "path": materialized_input_path,
            "source_path": source_input_path
        }));
        input_files.push(json!({
            "path": materialized_answer_path,
            "source_path": source_answer_path
        }));
        metadata_cases.push(json!({
            "case_name": case_name,
            "input_path": format!("cases/official-{}.in", safe_case_stem(&case.stem)),
            "answer_path": format!("answers/official-{}.ans", safe_case_stem(&case.stem))
        }));
    }
    metadata.insert("cases".to_string(), Value::Array(metadata_cases));
    let session = json!({
        "session_name": "official",
        "input_files": input_files,
        "metadata": metadata
    });
    entries.insert(PRIVATE_SESSION_PATH.to_string(), json_bytes(session)?);
    Ok(GeneratedOverlay {
        adapter_label: "piped-moving-mole-file-backed",
        case_count: cases.len(),
        selection_note: None,
        entries,
    })
}

fn generate_uniform_cave_session(cases: &[CaseFile]) -> anyhow::Result<GeneratedOverlay> {
    let selected = select_best_case(cases, |case| {
        let tokens = parse_i64_tokens(&read_utf8(&case.input_path)?);
        let (n, m) = first_two_i64(&tokens, "uniform cave input")?;
        n.checked_mul(m)
            .ok_or_else(|| anyhow!("uniform cave n*m overflows"))
    })?;
    let tokens = parse_i64_tokens(&read_utf8(&selected.input_path)?);
    let (n, m) = first_two_i64(&tokens, "uniform cave input")?;
    let cell_count = n
        .checked_mul(m)
        .ok_or_else(|| anyhow!("uniform cave n*m overflows"))?;
    let expected = usize::try_from(
        cell_count
            .checked_add(2)
            .ok_or_else(|| anyhow!("uniform cave token count overflows"))?,
    )
    .context("uniform cave token count must fit usize")?;
    if tokens.len() != expected {
        anyhow::bail!("uniform cave input matrix size does not match n*m");
    }
    let width = usize::try_from(m).context("uniform cave m must fit usize")?;
    let matrix_tokens = tokens
        .get(2..)
        .ok_or_else(|| anyhow!("uniform cave input is missing matrix"))?;
    let rows = matrix_tokens
        .chunks(width)
        .map(|row| Value::Array(row.iter().copied().map(Value::from).collect()))
        .collect::<Vec<_>>();
    let session = json!({
        "session_name": "official",
        "metadata": {
            "case": {
                "n": n,
                "m": m,
                "adj": rows
            }
        }
    });
    single_session_overlay("piped-uniform-cave", session)
}

fn generate_duplicate_position_runtime_session() -> anyhow::Result<GeneratedOverlay> {
    single_session_overlay(
        "piped-runtime-random-duplicate-position",
        json!({
            "session_name": "official",
            "metadata": {
                "runtime_random": {
                    "kind": "duplicate_position_search",
                    "case_count": 20,
                    "n": 300
                }
            }
        }),
    )
}

fn generate_limited_shuffle_runtime_session(
    cases: &[CaseFile],
) -> anyhow::Result<GeneratedOverlay> {
    let selected = select_best_case(cases, |case| {
        parse_i64_tokens(&read_utf8(&case.input_path)?)
            .first()
            .copied()
            .ok_or_else(|| anyhow!("limited shuffle input is empty"))
    })?;
    let n = parse_i64_tokens(&read_utf8(&selected.input_path)?)
        .first()
        .copied()
        .ok_or_else(|| anyhow!("limited shuffle input is empty"))?;
    single_session_overlay(
        "piped-runtime-random-limited-shuffle",
        json!({
            "session_name": "official",
            "metadata": {
                "runtime_random": {
                    "kind": "limited_shuffle_restore",
                    "n": n
                }
            }
        }),
    )
}

fn generate_signed_rooted_runtime_session(cases: &[CaseFile]) -> anyhow::Result<GeneratedOverlay> {
    let mut metadata_cases = Vec::new();
    for case in cases {
        metadata_cases.extend(parse_signed_rooted_cases(&read_utf8(&case.input_path)?)?);
    }
    single_session_overlay(
        "piped-runtime-random-signed-rooted-tree",
        json!({
            "session_name": "official",
            "metadata": {
                "runtime_random": {"kind": "signed_rooted_tree"},
                "cases": metadata_cases
            }
        }),
    )
}

fn generate_treasure_hunt_runtime_session(cases: &[CaseFile]) -> anyhow::Result<GeneratedOverlay> {
    let mut metadata_cases = Vec::new();
    for case in cases {
        metadata_cases.extend(parse_treasure_hunt_cases(&read_utf8(&case.input_path)?)?);
    }
    single_session_overlay(
        "piped-runtime-random-treasure-hunt",
        json!({
            "session_name": "official",
            "metadata": {
                "runtime_random": {"kind": "treasure_hunt_choices"},
                "cases": metadata_cases
            }
        }),
    )
}

fn single_session_overlay(
    adapter_label: &'static str,
    session: Value,
) -> anyhow::Result<GeneratedOverlay> {
    let case_count = session_case_count(&session);
    let mut entries = BTreeMap::new();
    entries.insert(PRIVATE_SESSION_PATH.to_string(), json_bytes(session)?);
    Ok(GeneratedOverlay {
        adapter_label,
        case_count,
        selection_note: None,
        entries,
    })
}

fn session_case_count(session: &Value) -> usize {
    let metadata = session.get("metadata").and_then(Value::as_object);
    if let Some(count) = metadata
        .and_then(|metadata| metadata.get("runtime_random"))
        .and_then(Value::as_object)
        .and_then(|policy| policy.get("case_count"))
        .and_then(Value::as_u64)
        .and_then(|count| usize::try_from(count).ok())
    {
        return count;
    }
    metadata
        .and_then(|metadata| metadata.get("cases"))
        .and_then(Value::as_array)
        .map_or(1, Vec::len)
}

fn parse_signed_rooted_cases(input: &str) -> anyhow::Result<Vec<Value>> {
    let tokens = parse_i64_tokens(input);
    let mut cursor = 0usize;
    let t = next_usize(&tokens, &mut cursor, "T")?;
    let mut cases = Vec::with_capacity(t);
    for _ in 0..t {
        cases.push(parse_signed_rooted_case(&tokens, &mut cursor)?);
    }
    while cursor < tokens.len() {
        cases.push(parse_signed_rooted_case(&tokens, &mut cursor)?);
    }
    Ok(cases)
}

fn parse_signed_rooted_case(tokens: &[i64], cursor: &mut usize) -> anyhow::Result<Value> {
    let n = next_usize(tokens, cursor, "n")?;
    let mut edges = Vec::with_capacity(n.saturating_sub(1));
    for _ in 0..n.saturating_sub(1) {
        let u = next_i64(tokens, cursor, "u")?;
        let v = next_i64(tokens, cursor, "v")?;
        edges.push(json!([u, v]));
    }
    let root = next_i64(tokens, cursor, "root")?;
    Ok(json!({ "n": n, "edges": edges, "root": root }))
}

fn parse_treasure_hunt_cases(input: &str) -> anyhow::Result<Vec<Value>> {
    let tokens = parse_i64_tokens(input);
    let mut cursor = 0usize;
    let t = next_usize(&tokens, &mut cursor, "T")?;
    let mut cases = Vec::with_capacity(t);
    for _ in 0..t {
        let n = next_usize(&tokens, &mut cursor, "n")?;
        let m = next_usize(&tokens, &mut cursor, "m")?;
        let start = next_i64(&tokens, &mut cursor, "start")?;
        let base_move_count = next_i64(&tokens, &mut cursor, "base_move_count")?;
        let mut edges = Vec::with_capacity(m);
        for _ in 0..m {
            let u = next_i64(&tokens, &mut cursor, "u")?;
            let v = next_i64(&tokens, &mut cursor, "v")?;
            edges.push(json!([u, v]));
        }
        cases.push(json!({
            "n": n,
            "m": m,
            "start": start,
            "base_move_count": base_move_count,
            "edges": edges
        }));
    }
    if cursor != tokens.len() {
        anyhow::bail!("treasure-hunt input contains extra tokens");
    }
    Ok(cases)
}

fn split_world_map_cases(input: &str) -> anyhow::Result<Vec<String>> {
    let tokens = parse_i64_tokens(input);
    let mut cursor = 0usize;
    let t = next_usize(&tokens, &mut cursor, "T")?;
    let mut cases = Vec::with_capacity(t);
    for _ in 0..t {
        let n = next_i64(&tokens, &mut cursor, "n")?;
        let m = next_usize(&tokens, &mut cursor, "m")?;
        let mut parts = vec![n.to_string(), m.to_string()];
        for _ in 0..m {
            parts.push(next_i64(&tokens, &mut cursor, "u")?.to_string());
            parts.push(next_i64(&tokens, &mut cursor, "v")?.to_string());
        }
        cases.push(format!("{}\n", parts.join(" ")));
    }
    if cursor != tokens.len() {
        anyhow::bail!("world-map input contains extra tokens");
    }
    Ok(cases)
}

struct CaseSelection {
    cases: Vec<CaseFile>,
    note: Option<String>,
}

fn max_challenge_runs_per_evaluation() -> anyhow::Result<usize> {
    usize::try_from(MAX_CHALLENGE_RUNS_PER_EVALUATION).context("challenge run limit must fit usize")
}

fn select_cases(cases: &[CaseFile], limit: usize) -> anyhow::Result<CaseSelection> {
    if cases.len() <= limit {
        return Ok(CaseSelection {
            cases: cases.to_vec(),
            note: None,
        });
    }
    let indexes = evenly_spaced_indexes(cases.len(), limit)?;
    let selected_cases = indexes
        .into_iter()
        .map(|index| {
            cases
                .get(index)
                .cloned()
                .ok_or_else(|| anyhow!("selected case index {index} is out of range"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(CaseSelection {
        cases: selected_cases,
        note: Some(format!(
            "selected {limit} of {} upstream cases",
            cases.len()
        )),
    })
}

struct NamedSelection {
    values: Vec<(String, String)>,
    note: Option<String>,
}

fn select_named_values(
    values: Vec<(String, String)>,
    limit: usize,
) -> anyhow::Result<NamedSelection> {
    if values.len() <= limit {
        return Ok(NamedSelection { values, note: None });
    }
    let indexes = evenly_spaced_indexes(values.len(), limit)?;
    let selected_values = indexes
        .into_iter()
        .map(|index| {
            values
                .get(index)
                .cloned()
                .ok_or_else(|| anyhow!("selected expanded case index {index} is out of range"))
        })
        .collect::<anyhow::Result<Vec<_>>>()?;
    Ok(NamedSelection {
        values: selected_values,
        note: Some(format!(
            "selected {limit} of {} expanded upstream cases",
            values.len()
        )),
    })
}

fn evenly_spaced_indexes(total: usize, limit: usize) -> anyhow::Result<Vec<usize>> {
    if limit == 0 {
        return Ok(Vec::new());
    }
    if limit == 1 {
        return Ok(vec![0]);
    }
    let total_span = total
        .checked_sub(1)
        .ok_or_else(|| anyhow!("case selection total underflow"))?;
    let limit_span = limit
        .checked_sub(1)
        .ok_or_else(|| anyhow!("case selection limit underflow"))?;
    let mut indexes = BTreeSet::new();
    for i in 0..limit {
        let numerator = i
            .checked_mul(total_span)
            .ok_or_else(|| anyhow!("case selection index overflow"))?;
        let index = numerator
            .checked_div(limit_span)
            .ok_or_else(|| anyhow!("case selection divisor is zero"))?;
        indexes.insert(index);
    }
    Ok(indexes.into_iter().collect())
}

fn select_best_case<F>(cases: &[CaseFile], score: F) -> anyhow::Result<CaseFile>
where
    F: Fn(&CaseFile) -> anyhow::Result<i64>,
{
    let mut best: Option<(i64, CaseFile)> = None;
    for case in cases {
        let value = score(case)?;
        if best
            .as_ref()
            .is_none_or(|(best_value, _)| value > *best_value)
        {
            best = Some((value, case.clone()));
        }
    }
    best.map(|(_, case)| case)
        .ok_or_else(|| anyhow!("no upstream testdata cases found"))
}

fn collect_case_files(problem_root: &Path) -> anyhow::Result<Vec<CaseFile>> {
    let testdata = problem_root.join("testdata");
    if !testdata.is_dir() {
        anyhow::bail!("missing upstream testdata dir {}", testdata.display());
    }
    let mut stems = BTreeSet::new();
    for entry in fs::read_dir(&testdata)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|value| value.to_str()) == Some("in")
            && let Some(stem) = path.file_stem().and_then(|value| value.to_str())
        {
            stems.insert(stem.to_string());
        }
    }
    let mut cases = Vec::with_capacity(stems.len());
    for stem in stems {
        let input_path = testdata.join(format!("{stem}.in"));
        let answer_path = testdata.join(format!("{stem}.ans"));
        if !answer_path.is_file() {
            anyhow::bail!("missing answer file {}", answer_path.display());
        }
        cases.push(CaseFile {
            stem,
            input_path,
            answer_path,
        });
    }
    cases.sort_by(|left, right| compare_case_stems(&left.stem, &right.stem));
    if cases.is_empty() {
        anyhow::bail!("no .in/.ans upstream testdata cases found");
    }
    Ok(cases)
}

fn compare_case_stems(left: &str, right: &str) -> std::cmp::Ordering {
    match (left.parse::<u64>(), right.parse::<u64>()) {
        (Ok(left), Ok(right)) => left.cmp(&right),
        _ => left.cmp(right),
    }
}

fn required_private_paths(manifest: &Value) -> anyhow::Result<Vec<String>> {
    let assets = manifest
        .get("private_assets")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("agentics.challenge.json missing private_assets"))?;
    let mut paths = Vec::new();
    for asset in assets {
        let Some(asset) = asset.as_object() else {
            continue;
        };
        if asset.get("required").and_then(Value::as_bool) != Some(true) {
            continue;
        }
        let required_paths = asset
            .get("required_paths")
            .and_then(Value::as_array)
            .ok_or_else(|| anyhow!("required private asset is missing required_paths"))?;
        for path in required_paths {
            let path = path
                .as_str()
                .ok_or_else(|| anyhow!("required private asset path must be a string"))?;
            paths.push(path.to_string());
        }
    }
    if paths.is_empty() {
        anyhow::bail!("required private assets declare no required paths");
    }
    Ok(paths)
}

fn problem_id_from_challenge_name(challenge_name: &str) -> anyhow::Result<u32> {
    challenge_name
        .rsplit_once("-algorithmic-")
        .and_then(|(_, id)| id.parse::<u32>().ok())
        .ok_or_else(|| {
            anyhow!("challenge name `{challenge_name}` does not end with algorithmic id")
        })
}

fn load_json_file(path: &Path) -> anyhow::Result<Value> {
    let content =
        fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))?;
    serde_json::from_str(&content).with_context(|| format!("failed to parse {}", path.display()))
}

fn required_object<'a>(value: &'a Value, field: &str) -> anyhow::Result<&'a Map<String, Value>> {
    value
        .get(field)
        .and_then(Value::as_object)
        .ok_or_else(|| anyhow!("missing object field `{field}`"))
}

fn required_array<'a>(value: &'a Value, field: &str) -> anyhow::Result<&'a Vec<Value>> {
    value
        .get(field)
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow!("missing array field `{field}`"))
}

fn required_str<'a>(value: &'a Map<String, Value>, field: &str) -> anyhow::Result<&'a str> {
    value
        .get(field)
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing string field `{field}`"))
}

fn json_bytes(value: Value) -> anyhow::Result<Vec<u8>> {
    serde_json::to_vec_pretty(&value).context("failed to serialize generated JSON")
}

fn read_utf8(path: &Path) -> anyhow::Result<String> {
    fs::read_to_string(path).with_context(|| format!("failed to read {}", path.display()))
}

fn parse_i64_tokens(text: &str) -> Vec<i64> {
    text.split_whitespace()
        .filter_map(|token| token.parse::<i64>().ok())
        .collect()
}

fn first_two_i64(tokens: &[i64], label: &str) -> anyhow::Result<(i64, i64)> {
    let first = tokens
        .first()
        .copied()
        .ok_or_else(|| anyhow!("{label} is missing first token"))?;
    let second = tokens
        .get(1)
        .copied()
        .ok_or_else(|| anyhow!("{label} is missing second token"))?;
    Ok((first, second))
}

fn next_i64(tokens: &[i64], cursor: &mut usize, label: &str) -> anyhow::Result<i64> {
    let value = tokens
        .get(*cursor)
        .copied()
        .ok_or_else(|| anyhow!("missing token `{label}`"))?;
    *cursor = (*cursor)
        .checked_add(1)
        .ok_or_else(|| anyhow!("token cursor overflow"))?;
    Ok(value)
}

fn next_usize(tokens: &[i64], cursor: &mut usize, label: &str) -> anyhow::Result<usize> {
    let value = next_i64(tokens, cursor, label)?;
    usize::try_from(value).with_context(|| format!("token `{label}` must be nonnegative"))
}

fn safe_case_stem(stem: &str) -> String {
    stem.chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' {
                ch.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect()
}
