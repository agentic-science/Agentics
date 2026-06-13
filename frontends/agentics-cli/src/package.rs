use std::path::Path;

use agentics_contracts::zip_project::{
    ZipProjectWorkspacePackage, package_zip_project_workspace as package_workspace,
};
use anyhow::Result;

pub(crate) type SolutionPackage = ZipProjectWorkspacePackage;

/// Package a local solution workspace using the shared `zip_project` policy.
pub(crate) fn package_solution_workspace(workspace_dir: &Path) -> Result<SolutionPackage> {
    Ok(package_workspace(workspace_dir)?)
}
