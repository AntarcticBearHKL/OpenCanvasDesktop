//! ZIP export/import for the library: the `CanvasExportFile` envelope
//! (`app: "open-canvas"`, `version: 3`) packed with its media, plus the
//! path-prompt modal state both flows share.
//!
//! Export collects every inline bitmap the core's `collect_export_assets` finds
//! (image/video node content, image-stack entries, video posters), appends the
//! studio projects an assets node references as whole JSON documents, packs the
//! archive with `pack_export`, and writes it to the typed path (`.zip`).
//! Import unpacks it, re-inlines the `archive:` references
//! (`restore_export_assets`), materialises the carried studio projects into the
//! library (re-minting colliding ids and remapping the references), and adds the
//! canvas projects.
//!
//! Packing a project with media is CPU-bound (base64 + CRC + ZIP) and the app
//! has no native file dialog, so the work runs on a worker thread: the modal
//! stages the job, shows its packing state, and `poll_archive` (drained by the
//! app tick through `poll_ai`) applies the result on the UI thread. The tests
//! drive that same path headlessly (`export_job` + `start_archive_job` +
//! `take_archive_done` + `apply_import`).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, Sender};

use open_canvas_core::document::{
    from_json, CanvasExportAsset, CanvasExportFile, CanvasGroup, CanvasProject,
    CanvasProjectExportItem, ProjectIndexEntry, EXPORT_APP, EXPORT_VERSION,
};
use open_canvas_core::node::CanvasNodeType;
use open_canvas_core::{
    collect_export_assets, pack_export, restore_export_assets, unpack_export, Blob,
};

use crate::app::App;
use crate::library::ExportFlow;
use crate::store::{self, Store};
use crate::AppWindow;

/// `CanvasExportAsset.storage_key` prefix marking a whole studio project
/// document (not one bitmap).
pub const STUDIO_KEY_PREFIX: &str = "studio:";
/// Media type the studio project documents travel as.
pub const STUDIO_MIME: &str = "application/json";

/// The studio project a canvas assets node can reference
/// (`CanvasAssetStudioProjects`): the Image Studio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StudioKind {
    Image,
}

impl StudioKind {
    /// Wire id used in the storage key and the archive member path.
    pub fn id(self) -> &'static str {
        match self {
            Self::Image => "image",
        }
    }

    /// Parses the wire id; anything else is not a studio kind.
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "image" => Some(Self::Image),
            _ => None,
        }
    }

    /// The library index kind a materialised project gets.
    pub fn project_kind(self) -> &'static str {
        match self {
            Self::Image => crate::image::PROJECT_KIND_IMAGE,
        }
    }
}

/// One referenced studio project carried by an archive: the raw
/// `<data-root>/images/<id>.json` bytes plus the member path they take.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StudioAsset {
    pub kind: StudioKind,
    pub id: String,
    pub path: String,
    pub bytes: Vec<u8>,
}

/// A packed export: the ZIP bytes plus what went into it (for the result line).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportedArchive {
    pub bytes: Vec<u8>,
    /// Inline bitmap blobs collected from the project JSON.
    pub images: usize,
    /// Studio project documents carried along.
    pub studio: usize,
    /// Referenced studio projects that could not be read; they are left out and
    /// reported instead of failing the export.
    pub missing: Vec<String>,
}

impl ExportedArchive {
    /// The user-facing result line for a written archive.
    pub fn summary(&self, path: &Path) -> String {
        let mut parts = vec![format!(
            "Exported {} image{} to {}",
            self.images,
            if self.images == 1 { "" } else { "s" },
            path.display()
        )];
        if self.studio > 0 {
            parts.push(format!(
                "· {} studio project{}",
                self.studio,
                if self.studio == 1 { "" } else { "s" }
            ));
        }
        parts.extend(
            self.missing
                .iter()
                .map(|missing| format!("· skipped: {missing}")),
        );
        parts.join(" ")
    }
}

/// What one import did (for the result line).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportSummary {
    pub projects: usize,
    pub studio: usize,
    /// Studio projects that could not be materialised, one sentence each.
    pub warnings: Vec<String>,
}

impl ImportSummary {
    /// The user-facing result line, warnings included.
    pub fn message(&self) -> String {
        let mut parts = vec![format!(
            "Imported {} project{}",
            self.projects,
            if self.projects == 1 { "" } else { "s" }
        )];
        if self.studio > 0 {
            parts.push(format!(
                "· {} studio project{}",
                self.studio,
                if self.studio == 1 { "" } else { "s" }
            ));
        }
        parts.extend(
            self.warnings
                .iter()
                .map(|warning| format!("· skipped: {warning}")),
        );
        parts.join(" ")
    }
}

/// The storage key of one studio project member (`studio:<kind>:<id>`).
pub fn studio_key(kind: StudioKind, id: &str) -> String {
    format!("{STUDIO_KEY_PREFIX}{}:{id}", kind.id())
}

/// Splits a studio storage key back into its kind and project id.
pub fn parse_studio_key(key: &str) -> Option<(StudioKind, String)> {
    let rest = key.strip_prefix(STUDIO_KEY_PREFIX)?;
    let (kind, id) = rest.split_once(':')?;
    let kind = StudioKind::from_id(kind)?;
    if id.is_empty() {
        return None;
    }
    Some((kind, id.to_string()))
}

/// The studio project ids an exported project's assets nodes reference.
fn studio_references(project: &CanvasProject) -> Vec<(StudioKind, String)> {
    let mut references = Vec::new();
    for node in &project.nodes {
        if node.node_type != CanvasNodeType::Assets {
            continue;
        }
        let Some(selection) = node
            .metadata
            .as_ref()
            .and_then(|meta| meta.asset_studio_projects.as_ref())
        else {
            continue;
        };
        for id in selection.image.iter().flatten() {
            if !id.is_empty() {
                references.push((StudioKind::Image, id.clone()));
            }
        }
    }
    references
}

/// One archive member path component: alphanumerics, `-` and `_` kept, runs of
/// anything else collapsed to `_` (the store's id-safety rule, applied to ids
/// that may come from another machine).
fn sanitize_component(value: &str) -> String {
    let mut out = String::new();
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
            out.push(ch);
        } else if !out.ends_with('_') {
            out.push('_');
        }
    }
    let out = out.trim_matches('_');
    if out.is_empty() {
        "project".to_string()
    } else {
        out.chars().take(64).collect()
    }
}

/// Packs one project's archive: collect the inline bitmaps, attach the studio
/// projects the project references, pack the ZIP. Shared by the UI-thread core
/// ([`App::export_archive`]) and the archive worker.
fn pack_project_archive(
    store: &Store,
    project: CanvasProject,
    group_name: Option<String>,
    exported_at: String,
) -> Result<ExportedArchive, String> {
    let export = CanvasExportFile {
        app: EXPORT_APP.to_string(),
        version: EXPORT_VERSION,
        exported_at,
        projects: vec![CanvasProjectExportItem {
            project,
            group_name,
            files: Vec::new(),
        }],
    };
    let collected =
        collect_export_assets(&export).map_err(|error| format!("Export failed: {error}"))?;
    let images = collected.blobs.len();
    let mut export = collected.export;
    let mut blobs = collected.blobs;
    let mut missing = Vec::new();
    let mut studio = 0;
    let Some(item) = export.projects.first_mut() else {
        return Err("Export failed: the envelope carries no project".to_string());
    };
    let project_component = sanitize_component(&item.project.id);
    let mut seen: BTreeSet<(StudioKind, String)> = BTreeSet::new();
    for (kind, id) in studio_references(&item.project) {
        if !seen.insert((kind, id.clone())) {
            continue;
        }
        let Some(path) = studio_path(store, kind, &id) else {
            missing.push(format!("studio {} {id}", kind.id()));
            continue;
        };
        let Ok(bytes) = store::read_bytes(&path) else {
            missing.push(format!("studio {} {id}", kind.id()));
            continue;
        };
        let member = format!(
            "projects/{project_component}/studio/{}-{}.json",
            kind.id(),
            sanitize_component(&id)
        );
        item.files.push(CanvasExportAsset {
            storage_key: studio_key(kind, &id),
            path: member.clone(),
            mime_type: STUDIO_MIME.to_string(),
            bytes: u64::try_from(bytes.len()).unwrap_or(u64::MAX),
        });
        blobs.push(Blob {
            path: member,
            mime_type: STUDIO_MIME.to_string(),
            data: bytes,
        });
        studio += 1;
    }
    let bytes = pack_export(&export, &blobs).map_err(|error| format!("Export failed: {error}"))?;
    Ok(ExportedArchive {
        bytes,
        images,
        studio,
        missing,
    })
}

/// `<data-root>/images/<id>.json` for a referenced studio project; an
/// unsafe id (or a foreign project id) resolves to `None`.
fn studio_path(store: &Store, kind: StudioKind, id: &str) -> Option<PathBuf> {
    match kind {
        StudioKind::Image => store.image_path(id).ok(),
    }
}

/// Unpacks an archive: the envelope with its `archive:` references inlined
/// again plus the studio project documents it carries. Shared by the worker and
/// the headless core.
fn unpack_archive(bytes: &[u8]) -> Result<(CanvasExportFile, Vec<StudioAsset>), String> {
    let (export, blobs) =
        unpack_export(bytes).map_err(|error| format!("Import failed: {error}"))?;
    if export.projects.is_empty() {
        return Err("The file contains no projects".to_string());
    }
    // The studio members are recognised by their manifest storage key; the blob
    // itself only carries the path, so the two are joined back here.
    let mut studio = Vec::new();
    for item in &export.projects {
        for file in &item.files {
            let Some((kind, id)) = parse_studio_key(&file.storage_key) else {
                continue;
            };
            let Some(blob) = blobs.iter().find(|blob| blob.path == file.path) else {
                continue;
            };
            studio.push(StudioAsset {
                kind,
                id,
                path: blob.path.clone(),
                bytes: blob.data.clone(),
            });
        }
    }
    let export = restore_export_assets(&export, &blobs)
        .map_err(|error| format!("Import failed: {error}"))?;
    Ok((export, studio))
}

/// The identity fields a carried studio project document holds.
#[derive(Debug, Clone, PartialEq, Eq)]
struct StudioMeta {
    id: String,
    title: String,
    created_at: String,
    updated_at: String,
}

/// Reads a carried studio document's identity; a document that does not parse
/// (or is not the declared kind) is `None`.
fn studio_meta(asset: &StudioAsset) -> Option<StudioMeta> {
    match asset.kind {
        StudioKind::Image => {
            let file: crate::image::ImageFile = serde_json::from_slice(&asset.bytes).ok()?;
            Some(StudioMeta {
                id: file.project.id,
                title: file.project.title,
                created_at: file.project.created_at,
                updated_at: file.project.updated_at,
            })
        }
    }
}

/// Rewrites the imported projects' assets-node references after a studio
/// project was re-materialised under a different id.
fn remap_studio_references(
    export: &mut CanvasExportFile,
    remap: &BTreeMap<(StudioKind, String), String>,
) {
    for item in &mut export.projects {
        for node in &mut item.project.nodes {
            if node.node_type != CanvasNodeType::Assets {
                continue;
            }
            let Some(selection) = node
                .metadata
                .as_mut()
                .and_then(|meta| meta.asset_studio_projects.as_mut())
            else {
                continue;
            };
            let Some(ids) = selection.image.as_mut() else {
                continue;
            };
            for id in ids.iter_mut() {
                if let Some(new_id) = remap.get(&(StudioKind::Image, id.clone())) {
                    id.clone_from(new_id);
                }
            }
        }
    }
}

/// One staged archive job: everything the worker thread needs, owned so it can
/// move across the thread boundary.
#[derive(Debug)]
enum ArchiveJob {
    Export {
        store: Store,
        path: PathBuf,
        /// The open editor's document, else the stored project file the worker
        /// loads (the read and parse stay off the UI thread).
        source: ProjectSource,
        group_name: Option<String>,
        exported_at: String,
    },
    Import {
        path: PathBuf,
    },
}

/// Where an exported project's document comes from.
#[derive(Debug)]
enum ProjectSource {
    /// The project open in the editor, already in memory.
    Inline(Box<CanvasProject>),
    /// A library project the worker loads from its file.
    Stored(PathBuf),
}

/// A finished job, handed from the worker back to the UI thread.
#[derive(Debug)]
enum ArchiveDone {
    Exported {
        summary: String,
    },
    Imported {
        export: Box<CanvasExportFile>,
        studio: Vec<StudioAsset>,
    },
    Failed(String),
}

/// The UI thread's archive worker: one job at a time (the modal shows its
/// packing state), results drained by the app tick.
struct ArchiveWorker {
    tx: Sender<ArchiveDone>,
    rx: Receiver<ArchiveDone>,
    /// The modal state the running job was started from, so its result only
    /// lands in that modal (a modal closed meanwhile gets a notice).
    flow: Option<ExportFlow>,
}

impl ArchiveWorker {
    fn new() -> Self {
        let (tx, rx) = mpsc::channel();
        Self { tx, rx, flow: None }
    }
}

thread_local! {
    /// One worker per UI thread (the app owns exactly one).
    static ARCHIVE: RefCell<ArchiveWorker> = RefCell::new(ArchiveWorker::new());
}

/// Stages `job` on the archive worker; `flow` is the modal state to report the
/// result to. An already running job is refused (the modal disables Run).
fn start_archive_job(job: ArchiveJob, flow: ExportFlow) -> Result<(), String> {
    ARCHIVE.with(|cell| {
        let mut worker = cell.borrow_mut();
        if worker.flow.is_some() {
            return Err("An export or import is still running".to_string());
        }
        let tx = worker.tx.clone();
        std::thread::Builder::new()
            .name("canvas-archive".to_string())
            .spawn(move || {
                let done = run_archive_job(job);
                let _sent = tx.send(done);
            })
            .map_err(|error| format!("The archive worker could not start ({error})"))?;
        worker.flow = Some(flow);
        Ok(())
    })
}

/// The finished job plus the modal state it was started from, if any.
fn take_archive_done() -> Option<(ArchiveDone, ExportFlow)> {
    ARCHIVE.with(|cell| {
        let mut worker = cell.borrow_mut();
        let done = worker.rx.try_recv().ok()?;
        let flow = worker.flow.take()?;
        Some((done, flow))
    })
}

/// Runs one job on the worker thread; every failure is a message for the modal.
fn run_archive_job(job: ArchiveJob) -> ArchiveDone {
    match job {
        ArchiveJob::Export {
            store,
            path,
            source,
            group_name,
            exported_at,
        } => {
            let project = match source {
                ProjectSource::Inline(project) => *project,
                ProjectSource::Stored(file) => {
                    let loaded = std::fs::read_to_string(&file)
                        .map_err(|error| error.to_string())
                        .and_then(|text| {
                            from_json::<CanvasProject>(&text).map_err(|error| error.to_string())
                        });
                    match loaded {
                        Ok(project) => project,
                        Err(error) => {
                            return ArchiveDone::Failed(format!("Project not readable: {error}"));
                        }
                    }
                }
            };
            match pack_project_archive(&store, project, group_name, exported_at) {
                Ok(archive) => match store::write_bytes(&path, &archive.bytes) {
                    Ok(()) => ArchiveDone::Exported {
                        summary: archive.summary(&path),
                    },
                    Err(error) => ArchiveDone::Failed(format!("Export failed: {error}")),
                },
                Err(message) => ArchiveDone::Failed(message),
            }
        }
        ArchiveJob::Import { path } => {
            let bytes = match store::read_bytes(&path) {
                Ok(bytes) => bytes,
                Err(error) => return ArchiveDone::Failed(format!("Import failed: {error}")),
            };
            match unpack_archive(&bytes) {
                Ok((export, studio)) => ArchiveDone::Imported {
                    export: Box::new(export),
                    studio,
                },
                Err(message) => ArchiveDone::Failed(message),
            }
        }
    }
}

impl App {
    pub fn open_import_modal(&mut self, ui: &AppWindow) {
        let path = self
            .store
            .root()
            .join("exports")
            .join("import.zip")
            .display()
            .to_string();
        self.library.export = Some(ExportFlow {
            import: true,
            project_id: String::new(),
            path,
            message: String::new(),
            error: false,
        });
        self.sync_library(ui);
    }

    pub fn export_import_close(&mut self, ui: &AppWindow) {
        self.library.export = None;
        self.sync_library(ui);
    }

    pub fn export_import_path_edited(&mut self, _ui: &AppWindow, path: &str) {
        if let Some(flow) = self.library.export.as_mut() {
            flow.path = path.to_string();
        }
    }

    /// Stages the open modal's flow on the archive worker; a failure to start
    /// (or an empty path) stays in the modal as an error line. The work runs off
    /// the UI thread, so the modal's packing state stays live and the result
    /// arrives through [`App::poll_archive`].
    pub fn export_import_run(&mut self, ui: &AppWindow) {
        let Some(flow) = self.library.export.clone() else {
            return;
        };
        let trimmed = flow.path.trim();
        if trimmed.is_empty() {
            self.set_export_message(ui, "Enter a file path first".to_string(), true);
            return;
        }
        let path = PathBuf::from(trimmed);
        let job = if flow.import {
            ArchiveJob::Import { path }
        } else {
            match self.export_job(&flow.project_id, path) {
                Ok(job) => job,
                Err(message) => {
                    self.set_export_message(ui, message, true);
                    return;
                }
            }
        };
        if let Err(message) = start_archive_job(job, flow) {
            self.set_export_message(ui, message, true);
        }
    }

    /// The export job for one project: the envelope's project (the open editor's
    /// document, else the stored file) plus the group name its card shows. The
    /// media collection and packing happen on the worker.
    fn export_job(&self, project_id: &str, path: PathBuf) -> Result<ArchiveJob, String> {
        let source = match self.editor.as_ref().filter(|meta| meta.id == project_id) {
            Some(meta) => ProjectSource::Inline(Box::new(self.doc.to_project(meta))),
            None => ProjectSource::Stored(
                self.store
                    .project_path(project_id)
                    .map_err(|error| format!("Project not readable: {error}"))?,
            ),
        };
        let group_name = self
            .library
            .index
            .project(project_id)
            .and_then(|row| row.group_id.as_deref())
            .and_then(|id| self.library.index.group(id))
            .map(|group| group.name.clone());
        Ok(ArchiveJob::Export {
            store: self.store.clone(),
            path,
            source,
            group_name,
            exported_at: store::now_iso(),
        })
    }

    /// Drains the archive worker (called from the app tick via `poll_ai`): a
    /// finished export/import is applied here, on the UI thread.
    pub fn poll_archive(&mut self, ui: &AppWindow) {
        let Some((done, flow)) = take_archive_done() else {
            return;
        };
        // The result only lands in the modal it was started from; a modal closed
        // (or restarted) meanwhile gets the notice instead.
        let in_modal = self.library.export.as_ref() == Some(&flow);
        match done {
            ArchiveDone::Failed(message) => {
                if in_modal {
                    self.set_export_message(ui, message, true);
                } else {
                    self.set_notice(ui, message, true);
                }
            }
            ArchiveDone::Exported { summary } => {
                if in_modal {
                    self.library.export = None;
                    self.clear_library_edits();
                }
                self.set_notice(ui, summary, false);
                self.sync_library(ui);
            }
            ArchiveDone::Imported { export, studio } => {
                match self.apply_import(*export, &studio) {
                    Ok(summary) => {
                        if in_modal {
                            self.library.export = None;
                            self.clear_library_edits();
                        }
                        self.set_notice(ui, summary.message(), false);
                    }
                    Err(message) => {
                        if in_modal {
                            self.set_export_message(ui, message, true);
                        } else {
                            self.set_notice(ui, message, true);
                        }
                    }
                }
                self.sync_library(ui);
            }
        }
    }

    /// Materialises one unpacked archive: studio projects first (their ids feed
    /// the reference remap), then the canvas projects. A missing target group is
    /// created by the envelope's `groupName`; an id that is already taken (or not
    /// a safe file name) is re-minted so nothing is overwritten.
    fn apply_import(
        &mut self,
        mut export: CanvasExportFile,
        studio: &[StudioAsset],
    ) -> Result<ImportSummary, String> {
        let mut summary = ImportSummary::default();
        let remap = self.materialise_studio(studio, &mut summary);
        if !remap.is_empty() {
            remap_studio_references(&mut export, &remap);
        }
        summary.projects = export.projects.len();
        for item in export.projects {
            let mut project = item.project;
            if !store::project_id_is_safe(&project.id)
                || self.library.index.project(&project.id).is_some()
            {
                self.library.seq = self.library.seq.wrapping_add(1);
                project.id = format!("project-imported-{}-{}", self.now_ms(), self.library.seq);
            }
            let group_name = item
                .group_name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty());
            project.group_id = Some(self.ensure_import_group(group_name));
            self.store
                .save_project(&project)
                .map_err(|error| format!("Import failed: {error}"))?;
            self.library.index.upsert_project(&project);
        }
        self.persist_index_quiet();
        Ok(summary)
    }

    /// Writes the archive's studio projects into the library and returns the
    /// old-id -> new-id map for the ones that had to be re-minted.
    fn materialise_studio(
        &mut self,
        assets: &[StudioAsset],
        summary: &mut ImportSummary,
    ) -> BTreeMap<(StudioKind, String), String> {
        let mut remap = BTreeMap::new();
        for asset in assets {
            let Some(meta) = studio_meta(asset) else {
                summary.warnings.push(format!(
                    "studio {} {} could not be read",
                    asset.kind.id(),
                    asset.id
                ));
                continue;
            };
            let id = if store::project_id_is_safe(&meta.id)
                && self.library.index.project(&meta.id).is_none()
            {
                meta.id.clone()
            } else {
                self.library.seq = self.library.seq.wrapping_add(1);
                format!(
                    "{}-imported-{}-{}",
                    asset.kind.id(),
                    self.now_ms(),
                    self.library.seq
                )
            };
            let group_id = Some(self.ensure_import_group(None));
            if let Err(error) = self.save_studio_asset(asset, &meta, &id, group_id.as_deref()) {
                summary.warnings.push(format!(
                    "studio {} {} could not be saved ({error})",
                    asset.kind.id(),
                    asset.id
                ));
                continue;
            }
            self.library.index.upsert_entry(ProjectIndexEntry {
                id: id.clone(),
                title: meta.title.clone(),
                created_at: meta.created_at.clone(),
                updated_at: meta.updated_at.clone(),
                group_id,
                kind: asset.kind.project_kind().to_string(),
            });
            if id != asset.id {
                remap.insert((asset.kind, asset.id.clone()), id);
            }
            summary.studio += 1;
        }
        remap
    }

    /// Writes one carried studio project under `id`, in the importing library's
    /// group (the document is re-emitted so its inner id and group agree with
    /// the file name and the index row).
    fn save_studio_asset(
        &self,
        asset: &StudioAsset,
        meta: &StudioMeta,
        id: &str,
        group_id: Option<&str>,
    ) -> Result<(), store::StoreError> {
        match asset.kind {
            StudioKind::Image => {
                let mut file: crate::image::ImageFile = serde_json::from_slice(&asset.bytes)
                    .map_err(|error| {
                        store::StoreError::Image(format!("studio image {}: {error}", meta.id))
                    })?;
                file.project.id = id.to_string();
                // `ImageProject.group_id` is a plain string (empty = no group).
                file.project.group_id = group_id.unwrap_or_default().to_string();
                self.store.save_image(&file)
            }
        }
    }

    /// Group for an imported project: an existing one by name, else a new one
    /// (named after the envelope, "Imported" as the last resort).
    fn ensure_import_group(&mut self, name: Option<&str>) -> String {
        if let Some(name) = name {
            if let Some(group) = self
                .library
                .index
                .groups
                .iter()
                .find(|group| group.name == name)
            {
                return group.id.clone();
            }
            self.library.seq = self.library.seq.wrapping_add(1);
            let id = format!("group-imported-{}-{}", self.now_ms(), self.library.seq);
            self.library.index.groups.push(CanvasGroup {
                id: id.clone(),
                name: name.to_string(),
                created_at: store::now_iso(),
            });
            return id;
        }
        if let Some(id) = self.library.selected_group.clone() {
            return id;
        }
        if let Some(group) = self.library.index.groups.first() {
            return group.id.clone();
        }
        self.library.seq = self.library.seq.wrapping_add(1);
        let id = format!("group-imported-{}-{}", self.now_ms(), self.library.seq);
        self.library.index.groups.push(CanvasGroup {
            id: id.clone(),
            name: "Imported".to_string(),
            created_at: store::now_iso(),
        });
        id
    }

    fn set_export_message(&mut self, ui: &AppWindow, message: String, error: bool) {
        if let Some(flow) = self.library.export.as_mut() {
            flow.message = message;
            flow.error = error;
        }
        self.sync_library(ui);
    }

    /// Pushes the export/import modal state to the window.
    pub(crate) fn sync_export(&mut self, ui: &AppWindow) {
        let Some(flow) = self.library.export.as_ref() else {
            ui.set_export_open(false);
            return;
        };
        ui.set_export_open(true);
        ui.set_export_import_mode(flow.import);
        ui.set_export_title(
            if flow.import {
                "Import project archive"
            } else {
                "Export project archive"
            }
            .into(),
        );
        ui.set_export_path(flow.path.as_str().into());
        ui.set_export_message(flow.message.as_str().into());
        ui.set_export_error(flow.error);
        ui.set_export_hint(
            if flow.import {
                "Reads an open-canvas v3 ZIP archive (projects.json + projects/<id>/files/...) and adds its projects, carrying the studio projects they reference."
            } else {
                "Writes the open-canvas v3 ZIP archive: the project JSON plus every referenced bitmap, packed together."
            }
            .into(),
        );
    }
}

#[cfg(test)]
#[path = "library_export_tests.rs"]
mod tests;
