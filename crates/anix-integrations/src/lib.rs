mod bootstrap;
mod groq;
mod model_runtime;
mod offload;
mod recorded_tool;
mod test_lead;
pub use anix_core::{
    CodingExecutor, Tool, ToolContext, ToolDescriptor, ToolFuture, ToolInput, ToolOutput,
};
use anix_domain::*;
pub use bootstrap::*;
pub use model_runtime::*;
pub use offload::*;
pub use recorded_tool::*;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    fs::{self, OpenOptions},
    io::{Read, Write},
    path::{Component, Path, PathBuf},
    sync::Arc,
};
pub use test_lead::*;

fn tool(error: impl std::fmt::Display) -> Error {
    Error::Tool(error.to_string())
}
#[derive(Clone)]
pub struct Workspace {
    root: PathBuf,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StoredFile {
    pub path: String,
    pub content_hash: String,
    pub byte_size: u64,
}
impl Workspace {
    pub fn open(root: impl AsRef<Path>) -> Result<Self> {
        fs::create_dir_all(root.as_ref()).map_err(tool)?;
        let root = fs::canonicalize(root).map_err(tool)?;
        let workspace = Self { root };
        // One directory rename keeps the SQLite database, WAL and artifact paths
        // together. Never silently choose between two independently existing stores.
        let legacy = workspace.resolve(Path::new(".rn"))?;
        let canonical = workspace.resolve(Path::new(".anix"))?;
        if legacy.exists() {
            if canonical.exists() {
                return Err(Error::Conflict("Both legacy .rn and canonical .anix stores exist; reconcile them explicitly before opening the workspace".into()));
            }
            fs::rename(&legacy, &canonical).map_err(tool)?;
        }
        for directory in [
            ".anix",
            "sources",
            "papers",
            "data",
            "experiments",
            "figures",
            "notes",
            "reports",
        ] {
            workspace.create_directory(directory)?;
        }
        Ok(workspace)
    }
    pub fn database_path(&self) -> Result<PathBuf> {
        self.resolve(Path::new(".anix/state.sqlite"))
    }
    fn create_directory(&self, name: &str) -> Result<()> {
        let path = self.resolve(Path::new(name))?;
        fs::create_dir_all(path).map_err(tool)
    }
    fn resolve(&self, path: &Path) -> Result<PathBuf> {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
        {
            return Err(Error::PermissionDenied);
        }
        // Reject Windows alternate data streams, including when running on Unix.
        if path.to_string_lossy().contains(':') || path.to_string_lossy().contains('\\') {
            return Err(Error::PermissionDenied);
        }
        // Historical artifact references retain their recorded spelling/hash.
        let mapped;
        let path = if path.starts_with(".rn")
            && path.components().count() > 1
            && !self.root.join(".rn").exists()
            && self.root.join(".anix").exists()
        {
            mapped = Path::new(".anix").join(path.strip_prefix(".rn").map_err(tool)?);
            mapped.as_path()
        } else {
            path
        };
        let target = self.root.join(path);
        let mut current = self.root.clone();
        for component in path.components() {
            current.push(component);
            if let Ok(meta) = fs::symlink_metadata(&current) {
                if meta.file_type().is_symlink() {
                    return Err(Error::PermissionDenied);
                }
                let canonical = fs::canonicalize(&current).map_err(tool)?;
                if !canonical.starts_with(&self.root) {
                    return Err(Error::PermissionDenied);
                }
            }
        }
        Ok(target)
    }
    pub fn write_new(&self, relative: &str, bytes: &[u8]) -> Result<StoredFile> {
        let target = self.resolve(Path::new(relative))?;
        let parent = target.parent().ok_or(Error::PermissionDenied)?;
        fs::create_dir_all(parent).map_err(tool)?;
        let target = self.resolve(Path::new(relative))?;
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .map_err(tool)?;
        if let Err(e) = file.write_all(bytes).and_then(|_| file.sync_all()) {
            drop(file);
            let _ = fs::remove_file(&target);
            return Err(tool(e));
        }
        Ok(StoredFile {
            path: relative.into(),
            content_hash: format!("{:x}", Sha256::digest(bytes)),
            byte_size: bytes.len() as u64,
        })
    }
    pub fn read(&self, relative: &str, max_bytes: u64) -> Result<Vec<u8>> {
        let target = self.resolve(Path::new(relative))?;
        let file = fs::File::open(target).map_err(tool)?;
        if !file.metadata().map_err(tool)?.is_file() {
            return Err(Error::Validation("regular file required".into()));
        }
        let mut bytes = vec![];
        file.take(max_bytes.saturating_add(1))
            .read_to_end(&mut bytes)
            .map_err(tool)?;
        if bytes.len() as u64 > max_bytes {
            return Err(Error::Validation("file exceeds read limit".into()));
        }
        Ok(bytes)
    }
    pub fn verify(&self, file: &StoredFile) -> Result<bool> {
        let bytes = self.read(&file.path, file.byte_size)?;
        Ok(bytes.len() as u64 == file.byte_size
            && format!("{:x}", Sha256::digest(bytes)) == file.content_hash)
    }
    /// Generated projections have their own subtree; never overwrite artifacts
    /// or human notes. Replacements use a sibling file and rename.
    pub fn rebuild_projections(
        &self,
        hypotheses: &[Hypothesis],
        findings: &[Finding],
    ) -> Result<()> {
        for h in hypotheses {
            self.projection(
                &format!("notes/generated/hypotheses/{}.md", h.id),
                &format!(
                    "# {}\n\nStatus: {:?}; version: {}\n\n{}\n",
                    h.title, h.status, h.meta.version, h.statement
                ),
            )?;
        }
        for f in findings {
            self.projection(
                &format!("notes/generated/findings/{}.md", f.id),
                &format!(
                    "# {}\n\nExperiment run: {}\n\n{}\n",
                    f.title, f.experiment_run_id, f.summary
                ),
            )?;
        }
        Ok(())
    }
    fn projection(&self, relative: &str, text: &str) -> Result<()> {
        let target = self.resolve(Path::new(relative))?;
        fs::create_dir_all(target.parent().ok_or(Error::PermissionDenied)?).map_err(tool)?;
        let staged = format!("{relative}.{}.stage", ArtifactId::new());
        let stage = self.write_new(&staged, text.as_bytes())?;
        let stage_path = self.resolve(Path::new(&stage.path))?;
        // Windows rename cannot replace an existing file. These files are
        // rebuildable; removing a previous projection does not affect truth.
        if target.exists() {
            fs::remove_file(&target).map_err(tool)?;
        }
        fs::rename(stage_path, target).map_err(tool)
    }
}

pub struct ReadFileTool {
    project_id: ProjectId,
    workspace: Arc<Workspace>,
}
impl ReadFileTool {
    pub fn new(project_id: ProjectId, workspace: Arc<Workspace>) -> Self {
        Self {
            project_id,
            workspace,
        }
    }
}
impl Tool for ReadFileTool {
    fn descriptor(&self) -> ToolDescriptor {
        ToolDescriptor {
            name: "read_file".into(),
            description: "Read at most 1 MiB inside the project workspace".into(),
            read_only: true,
        }
    }
    fn execute(&self, input: ToolInput, context: ToolContext) -> ToolFuture<'_> {
        Box::pin(async move {
            if context.project_id != self.project_id {
                return Err(Error::ProjectMismatch);
            }
            if input.schema_version != 1 {
                return Err(Error::Validation("tool schema version".into()));
            }
            let path = input
                .parameters
                .get("path")
                .and_then(Value::as_str)
                .ok_or_else(|| Error::Validation("path required".into()))?;
            // Windows ignores case and trailing dots/spaces in path components.
            // Protect operational storage in nested workspaces as well as this root.
            if path.split('/').any(|component| {
                component
                    .trim_end_matches(['.', ' '])
                    .eq_ignore_ascii_case(".anix")
                    || component
                        .trim_end_matches([' ', '.'])
                        .eq_ignore_ascii_case(".rn")
            }) {
                return Err(Error::PermissionDenied);
            }
            let target = self.workspace.resolve(Path::new(path))?;
            let canonical = fs::canonicalize(target).map_err(tool)?;
            if canonical
                .strip_prefix(&self.workspace.root)
                .map_err(tool)?
                .components()
                .any(|component| {
                    component
                        .as_os_str()
                        .to_string_lossy()
                        .eq_ignore_ascii_case(".anix")
                        || component
                            .as_os_str()
                            .to_string_lossy()
                            .trim_end_matches([' ', '.'])
                            .eq_ignore_ascii_case(".rn")
                })
            {
                return Err(Error::PermissionDenied);
            }
            let bytes = self.workspace.read(path, 1_048_576)?;
            let content = String::from_utf8(bytes).map_err(tool)?;
            safe_metadata(&Value::String(content.clone()))?;
            Ok(ToolOutput {
                schema_version: 1,
                data: Value::String(content),
                metadata: serde_json::json!({"schema_version":1}),
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_workspace_moves_once_and_keeps_reference_resolution() {
        let temp = tempfile::tempdir().unwrap();
        fs::create_dir(temp.path().join(".rn")).unwrap();
        fs::write(
            temp.path().join(".rn/state.sqlite"),
            b"preserved database bytes",
        )
        .unwrap();
        let workspace = Workspace::open(temp.path()).unwrap();
        assert_eq!(
            workspace.database_path().unwrap(),
            fs::canonicalize(temp.path())
                .unwrap()
                .join(".anix/state.sqlite")
        );
        assert!(!temp.path().join(".rn").exists());
        assert_eq!(
            workspace.read(".rn/state.sqlite", 100).unwrap(),
            b"preserved database bytes"
        );
        Workspace::open(temp.path()).unwrap();
        assert_eq!(
            fs::read(temp.path().join(".anix/state.sqlite")).unwrap(),
            b"preserved database bytes"
        );
    }
    #[test]
    fn competing_stores_are_rejected_without_overwriting_either() {
        let temp = tempfile::tempdir().unwrap();
        for (directory, data) in [(".rn", b"old"), (".anix", b"new")] {
            fs::create_dir(temp.path().join(directory)).unwrap();
            fs::write(temp.path().join(directory).join("state.sqlite"), data).unwrap();
        }
        assert!(matches!(
            Workspace::open(temp.path()),
            Err(Error::Conflict(_))
        ));
        assert_eq!(
            fs::read(temp.path().join(".rn/state.sqlite")).unwrap(),
            b"old"
        );
        assert_eq!(
            fs::read(temp.path().join(".anix/state.sqlite")).unwrap(),
            b"new"
        );
    }
    #[test]
    fn file_tool_rejects_storage_case_aliases_and_nested_databases() {
        let temp = tempfile::tempdir().unwrap();
        let workspace = Arc::new(Workspace::open(temp.path()).unwrap());
        workspace
            .write_new(".anix/state.sqlite", b"private operational content")
            .unwrap();
        let project = ProjectId::new();
        let tool = ReadFileTool::new(project, workspace);
        for path in [
            ".anix/state.sqlite",
            ".RN/state.sqlite",
            ".ANIX/state.sqlite",
            ".AnIx. /state.sqlite",
            ".Rn. /state.sqlite",
            "data/nested/.anix/state.sqlite",
        ] {
            let result = futures::executor::block_on(tool.execute(
                ToolInput {
                    schema_version: 1,
                    parameters: serde_json::json!({"path":path}),
                },
                ToolContext {
                    project_id: project,
                    run_id: None,
                    context_snapshot_id: None,
                    tool_invocation_id: None,
                },
            ));
            assert!(
                matches!(result, Err(Error::PermissionDenied)),
                "{path}: {result:?}"
            );
        }
    }
    #[test]
    fn boundaries_and_hashes() {
        let temp = tempfile::tempdir().unwrap();
        let w = Workspace::open(temp.path()).unwrap();
        for path in [
            "../outside",
            "/absolute",
            "C:/escape",
            "data/../escape",
            "data\\escape",
        ] {
            assert!(w.write_new(path, b"x").is_err());
        }
        let f = w.write_new("data/test.csv", b"x,y\n1,2\n").unwrap();
        assert!(w.verify(&f).unwrap());
        assert!(w.write_new("data/test.csv", b"replace").is_err());
        assert!(w.read("data/test.csv", 1).is_err());
    }
}
