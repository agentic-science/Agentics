use crate::zip_project::ZipProjectPhaseName;

pub(super) fn append_phase_logs(logs: &mut String, phase: ZipProjectPhaseName, content: &str) {
    append_named_logs(logs, &format!("phase:{}", phase_name(&phase)), content);
}

pub(super) fn append_run_logs(logs: &mut String, run_id: &str, content: &str) {
    append_named_logs(logs, &format!("run:{run_id}"), content);
}

pub(super) fn append_named_logs(logs: &mut String, name: &str, content: &str) {
    logs.push_str("\n===== ");
    logs.push_str(name);
    logs.push_str(" =====\n");
    logs.push_str(content);
    if !content.ends_with('\n') {
        logs.push('\n');
    }
}

pub(super) fn append_log_excerpt(message: &str, logs: &str) -> String {
    let trimmed = logs.trim();
    if trimmed.is_empty() {
        return message.to_string();
    }
    let excerpt: String = trimmed.chars().take(500).collect();
    format!("{message}; logs: {excerpt}")
}

pub(super) fn phase_name(phase: &ZipProjectPhaseName) -> &'static str {
    match phase {
        ZipProjectPhaseName::Setup => "setup",
        ZipProjectPhaseName::Build => "build",
        ZipProjectPhaseName::Run => "run",
    }
}
