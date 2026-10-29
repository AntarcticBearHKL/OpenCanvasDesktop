//! Local model manager state: the two models the web ships descriptors for
//! (`background-removal` `ISNet`, `mobile-sam`) plus an honest scan of the
//! desktop's models folder.
//!
//! The web runs both models in the browser (`@imgly/background-removal` worker,
//! `onnxruntime-web` `MobileSAM` with Hugging Face downloads). This build ships
//! no local inference runtime, so the manager never offers a download: it lists
//! the real files found in `<data-root>\models`, reports [`runtime_available`]
//! as false, and the tab says so. When a runtime lands, only this module changes.

use std::path::{Path, PathBuf};

/// One known model: descriptor keys for the i18n tables + the files that make it
/// "installed".
pub struct LocalModelSpec {
    pub id: &'static str,
    pub name_key: &'static str,
    pub desc_key: &'static str,
    pub files: &'static [&'static str],
}

/// `LOCAL_MODEL_DESCRIPTORS` equivalent (`use-local-model-store.ts`).
pub const LOCAL_MODELS: [LocalModelSpec; 2] = [
    LocalModelSpec {
        id: "background-removal",
        name_key: "config.local.bg-name",
        desc_key: "config.local.bg-desc",
        files: &["isnet.onnx"],
    },
    LocalModelSpec {
        id: "mobile-sam",
        name_key: "config.local.sam-name",
        desc_key: "config.local.sam-desc",
        files: &["mobilesam.encoder.onnx", "mobilesam.decoder.onnx"],
    },
];

/// Scan result of one descriptor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalModelStatus {
    pub id: &'static str,
    pub installed: bool,
    pub bytes: u64,
}

/// Everything the Local models tab renders: the folder, the runtime flag and
/// the per-model file state.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalModelsView {
    pub dir: PathBuf,
    pub runtime_available: bool,
    pub models: Vec<LocalModelStatus>,
}

/// Whether this build can run local models. Hard `false` until an inference
/// runtime is ported; the UI renders the "no runtime" state from this flag.
pub fn runtime_available() -> bool {
    false
}

/// Scans `models_dir` for each descriptor's files: a model is installed only
/// when every required file exists as a regular file, and its size is the sum
/// of those files.
pub fn scan(models_dir: &Path) -> LocalModelsView {
    let models = LOCAL_MODELS
        .iter()
        .map(|spec| {
            let mut bytes = 0;
            let mut installed = true;
            for file in spec.files {
                match std::fs::metadata(models_dir.join(file)) {
                    Ok(meta) if meta.is_file() => bytes += meta.len(),
                    _ => installed = false,
                }
            }
            LocalModelStatus {
                id: spec.id,
                installed,
                bytes: if installed { bytes } else { 0 },
            }
        })
        .collect();
    LocalModelsView {
        dir: models_dir.to_path_buf(),
        runtime_available: runtime_available(),
        models,
    }
}

#[cfg(test)]
#[path = "local_models_tests.rs"]
mod tests;
