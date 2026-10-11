use crate::*;
use anix_core::{CommandContext, Core, NewArtifact};
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OffloadedOutput {
    Inline {
        data: Value,
    },
    Artifact {
        id: ArtifactId,
        path: String,
        content_hash: String,
        byte_size: u64,
        schema: Value,
        summary: String,
    },
}
pub struct ArtifactOffloader {
    pub workspace: Arc<Workspace>,
    pub threshold_bytes: usize,
}
impl ArtifactOffloader {
    pub fn store(
        &self,
        core: &Core,
        project: ProjectId,
        data: Value,
        schema: Value,
        summary: String,
        ctx: CommandContext,
    ) -> Result<OffloadedOutput> {
        let bytes = serde_json::to_vec(&data).map_err(tool)?;
        if bytes.len() <= self.threshold_bytes {
            return Ok(OffloadedOutput::Inline { data });
        }
        text(&summary, "offload summary")?;
        safe_metadata(&schema)?;
        let stored = self.workspace.write_new(
            &format!("data/tool-output-{}.json", ActionId::new()),
            &bytes,
        )?;
        let input = NewArtifact {
            name: "offloaded tool output".into(),
            artifact_kind: ArtifactKind::Output,
            path: stored.path.clone(),
            content_hash: stored.content_hash.clone(),
            byte_size: stored.byte_size,
            metadata: serde_json::json!({"schema":schema,"summary":summary}),
        };
        let artifact = if matches!(ctx.actor, Actor::Human(_)) {
            core.register_artifact(project, input, ctx)?
        } else {
            core.register_execution_artifact(project, input, ctx)?
        };
        Ok(OffloadedOutput::Artifact {
            id: artifact.id,
            path: stored.path,
            content_hash: stored.content_hash,
            byte_size: stored.byte_size,
            schema,
            summary,
        })
    }
    pub fn read_range(&self, output: &OffloadedOutput, start: u64, length: u64) -> Result<Vec<u8>> {
        use std::io::{Seek, SeekFrom};
        let OffloadedOutput::Artifact {
            path,
            content_hash,
            byte_size,
            ..
        } = output
        else {
            return Err(Error::Validation("range requires artifact".into()));
        };
        if length > 1_048_576 || start.checked_add(length).is_none_or(|end| end > *byte_size) {
            return Err(Error::Validation("artifact range bounds".into()));
        }
        if !self.workspace.verify(&StoredFile {
            path: path.clone(),
            content_hash: content_hash.clone(),
            byte_size: *byte_size,
        })? {
            return Err(Error::Validation("artifact hash changed".into()));
        }
        let mut file = fs::File::open(self.workspace.resolve(Path::new(path))?).map_err(tool)?;
        file.seek(SeekFrom::Start(start)).map_err(tool)?;
        let mut bytes = vec![0; length as usize];
        file.read_exact(&mut bytes).map_err(tool)?;
        Ok(bytes)
    }
}
