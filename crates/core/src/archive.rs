//! ZIP packaging for the `"open-canvas"` v3 export envelope.
//!
//! The web export writes a ZIP with the JSON manifest at `projects.json` and
//! every binary asset under `projects/<project-id>/files/...` (see
//! `web/src/lib/canvas/canvas-export.ts`). This module reproduces that member
//! layout with a minimal, dependency-free store-only ZIP writer/reader, and
//! validates the manifest against the archive members before returning either
//! side of a pack/unpack so a malformed archive never yields partial success.
//!
//! Blobs also carry their own `path`; `collect_export_assets` assigns those
//! paths and rewrites project JSON references to `archive:<path>` tokens, and
//! `restore_export_assets` inlines them back as data URLs.

use std::collections::{BTreeMap, BTreeSet};

use crate::archive_read::read_zip;
use crate::archive_wire::member_path_is_safe;
use crate::archive_write::write_zip;
use crate::document::{CanvasExportFile, DocumentError};

pub use crate::archive_assets::{
    archive_ref, collect_export_assets, restore_export_assets, ARCHIVE_REF_PREFIX,
};

/// Name of the manifest member every export archive carries.
pub const PROJECTS_MANIFEST: &str = "projects.json";

/// One binary asset stored in an export archive.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blob {
    /// Archive member path, e.g. `projects/p1/files/image_abc.png`.
    pub path: String,
    pub mime_type: String,
    pub data: Vec<u8>,
}

/// Result of [`collect_export_assets`]: the remapped envelope plus its blobs.
#[derive(Debug, Clone, PartialEq)]
pub struct CollectedExport {
    pub export: CanvasExportFile,
    pub blobs: Vec<Blob>,
}

/// Bounds applied while packing or unpacking an archive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ArchiveLimits {
    /// Maximum number of members (including `projects.json`).
    pub max_entries: usize,
    /// Maximum uncompressed bytes for one member.
    pub max_entry_bytes: u64,
    /// Maximum sum of uncompressed bytes across all members.
    pub max_total_bytes: u64,
}

impl Default for ArchiveLimits {
    fn default() -> Self {
        Self {
            max_entries: 4096,
            max_entry_bytes: 128 * 1024 * 1024,
            max_total_bytes: 512 * 1024 * 1024,
        }
    }
}

/// Typed failures for asset collection and ZIP packaging.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("not a valid ZIP archive: {0}")]
    InvalidZip(&'static str),
    #[error("truncated ZIP archive")]
    Truncated,
    #[error("unsupported ZIP feature: {0}")]
    Unsupported(&'static str),
    #[error("archive member {path:?} escapes the archive root")]
    UnsafeMemberPath { path: String },
    #[error("duplicate archive member {name:?}")]
    DuplicateMember { name: String },
    #[error("duplicate blob {path:?} in the manifest")]
    DuplicateBlob { path: String },
    #[error("archive member {name:?} is reserved")]
    ReservedMember { name: String },
    #[error("archive exceeds the {kind} limit of {limit}")]
    LimitExceeded { kind: &'static str, limit: u64 },
    #[error("archive is missing its {PROJECTS_MANIFEST:?} manifest")]
    MissingManifest,
    #[error("invalid export manifest: {0}")]
    Manifest(#[from] DocumentError),
    #[error("manifest references missing blob {path:?}")]
    MissingBlob { path: String },
    #[error("blob {path:?} is not referenced by the manifest")]
    UnreferencedBlob { path: String },
    #[error("manifest blob {path:?} declares {declared} bytes but carries {actual}")]
    SizeMismatch {
        path: String,
        declared: u64,
        actual: u64,
    },
    #[error("manifest blob {path:?} declares media type {declared:?} but carries {actual:?}")]
    MimeMismatch {
        path: String,
        declared: String,
        actual: String,
    },
    #[error("archive member {name:?} has checksum {actual:#010x}, expected {expected:#010x}")]
    CrcMismatch {
        name: String,
        expected: u32,
        actual: u32,
    },
    #[error("invalid base64 payload in a data URL")]
    InvalidBase64,
    #[error("invalid data URL: {0}")]
    InvalidDataUrl(&'static str),
}

/// Pack an export envelope and its blobs into a ZIP archive.
///
/// Every `files` entry in the manifest must have exactly one blob of the same
/// path, byte length, and media type; unreferenced blobs are rejected so a
/// caller cannot silently ship asset bytes the manifest does not describe.
pub fn pack_export(export: &CanvasExportFile, blobs: &[Blob]) -> Result<Vec<u8>, ArchiveError> {
    pack_export_with_limits(export, blobs, &ArchiveLimits::default())
}

/// [`pack_export`] with explicit size/count limits.
pub fn pack_export_with_limits(
    export: &CanvasExportFile,
    blobs: &[Blob],
    limits: &ArchiveLimits,
) -> Result<Vec<u8>, ArchiveError> {
    export.validate()?;

    let mut blob_by_path: BTreeMap<&str, &Blob> = BTreeMap::new();
    for blob in blobs {
        validate_member_path(&blob.path)?;
        if blob.path == PROJECTS_MANIFEST {
            return Err(ArchiveError::ReservedMember {
                name: blob.path.clone(),
            });
        }
        if blob_by_path.insert(blob.path.as_str(), blob).is_some() {
            return Err(ArchiveError::DuplicateBlob {
                path: blob.path.clone(),
            });
        }
    }

    let mut referenced: BTreeSet<&str> = BTreeSet::new();
    let mut ordered: Vec<(String, &[u8])> = Vec::new();
    for item in &export.projects {
        for file in &item.files {
            validate_member_path(&file.path)?;
            if file.path == PROJECTS_MANIFEST {
                return Err(ArchiveError::ReservedMember {
                    name: file.path.clone(),
                });
            }
            if !referenced.insert(file.path.as_str()) {
                return Err(ArchiveError::DuplicateBlob {
                    path: file.path.clone(),
                });
            }
            let Some(blob) = blob_by_path.get(file.path.as_str()) else {
                return Err(ArchiveError::MissingBlob {
                    path: file.path.clone(),
                });
            };
            let actual = blob.data.len() as u64;
            if actual != file.bytes {
                return Err(ArchiveError::SizeMismatch {
                    path: file.path.clone(),
                    declared: file.bytes,
                    actual,
                });
            }
            if !file.mime_type.is_empty() && file.mime_type != blob.mime_type {
                return Err(ArchiveError::MimeMismatch {
                    path: file.path.clone(),
                    declared: file.mime_type.clone(),
                    actual: blob.mime_type.clone(),
                });
            }
            ordered.push((file.path.clone(), blob.data.as_slice()));
        }
    }

    for blob in blobs {
        if !referenced.contains(blob.path.as_str()) {
            return Err(ArchiveError::UnreferencedBlob {
                path: blob.path.clone(),
            });
        }
    }

    let manifest = serde_json::to_vec_pretty(export).map_err(DocumentError::from)?;
    if manifest.len() as u64 > limits.max_entry_bytes {
        return Err(ArchiveError::LimitExceeded {
            kind: "entry byte",
            limit: limits.max_entry_bytes,
        });
    }

    let mut entries: Vec<(String, &[u8])> = Vec::with_capacity(ordered.len() + 1);
    entries.push((PROJECTS_MANIFEST.to_string(), manifest.as_slice()));
    entries.extend(ordered);
    write_zip(&entries, limits)
}

/// Unpack a ZIP archive into its export envelope and blobs.
///
/// The manifest is validated against the members (every listed file present,
/// byte counts matching) before anything is returned.
pub fn unpack_export(bytes: &[u8]) -> Result<(CanvasExportFile, Vec<Blob>), ArchiveError> {
    unpack_export_with_limits(bytes, &ArchiveLimits::default())
}

/// [`unpack_export`] with explicit size/count limits.
pub fn unpack_export_with_limits(
    bytes: &[u8],
    limits: &ArchiveLimits,
) -> Result<(CanvasExportFile, Vec<Blob>), ArchiveError> {
    let members = read_zip(bytes, limits)?;
    let manifest = members
        .iter()
        .find(|member| member.name == PROJECTS_MANIFEST)
        .ok_or(ArchiveError::MissingManifest)?;
    let export: CanvasExportFile =
        serde_json::from_slice(&manifest.data).map_err(DocumentError::from)?;
    export.validate()?;

    let mut by_name: BTreeMap<&str, &[u8]> = BTreeMap::new();
    for member in &members {
        by_name.insert(member.name.as_str(), member.data.as_slice());
    }

    let mut seen: BTreeSet<&str> = BTreeSet::new();
    let mut blobs: Vec<Blob> = Vec::new();
    for item in &export.projects {
        for file in &item.files {
            validate_member_path(&file.path)?;
            if file.path == PROJECTS_MANIFEST {
                return Err(ArchiveError::ReservedMember {
                    name: file.path.clone(),
                });
            }
            if !seen.insert(file.path.as_str()) {
                return Err(ArchiveError::DuplicateBlob {
                    path: file.path.clone(),
                });
            }
            let Some(data) = by_name.get(file.path.as_str()) else {
                return Err(ArchiveError::MissingBlob {
                    path: file.path.clone(),
                });
            };
            let actual = data.len() as u64;
            if actual != file.bytes {
                return Err(ArchiveError::SizeMismatch {
                    path: file.path.clone(),
                    declared: file.bytes,
                    actual,
                });
            }
            let mime_type = if file.mime_type.is_empty() {
                "application/octet-stream".to_string()
            } else {
                file.mime_type.clone()
            };
            blobs.push(Blob {
                path: file.path.clone(),
                mime_type,
                data: data.to_vec(),
            });
        }
    }
    Ok((export, blobs))
}

fn validate_member_path(name: &str) -> Result<(), ArchiveError> {
    if member_path_is_safe(name) {
        Ok(())
    } else {
        Err(ArchiveError::UnsafeMemberPath {
            path: name.to_string(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::CanvasProject;
    use crate::document::{CanvasExportAsset, CanvasProjectExportItem, EXPORT_APP, EXPORT_VERSION};

    fn project(id: &str) -> CanvasProject {
        CanvasProject::new(id, "Canvas", "2026-01-01T00:00:00Z", "2026-01-01T00:00:00Z")
    }

    fn export_with(files: Vec<CanvasExportAsset>) -> CanvasExportFile {
        CanvasExportFile {
            app: EXPORT_APP.to_string(),
            version: EXPORT_VERSION,
            exported_at: "2026-01-02T00:00:00Z".to_string(),
            projects: vec![CanvasProjectExportItem {
                project: project("p1"),
                group_name: None,
                files,
            }],
        }
    }

    fn asset(path: &str, mime: &str, bytes: u64) -> CanvasExportAsset {
        CanvasExportAsset {
            storage_key: "image:a".to_string(),
            path: path.to_string(),
            mime_type: mime.to_string(),
            bytes,
        }
    }

    #[test]
    fn files_less_export_round_trips_byte_identically() {
        let export = export_with(Vec::new());
        let json = serde_json::to_string(&export).unwrap();
        assert!(json.contains("\"files\":[]"));
        let back: CanvasExportFile = serde_json::from_str(&json).unwrap();
        assert_eq!(back, export);
        assert_eq!(serde_json::to_string(&back).unwrap(), json);

        // Collection leaves a files-less envelope untouched.
        let collected = collect_export_assets(&export).unwrap();
        assert!(collected.blobs.is_empty());
        assert_eq!(collected.export, export);
        assert_eq!(serde_json::to_string(&collected.export).unwrap(), json);
    }

    #[test]
    fn pack_unpack_round_trip_preserves_manifest_and_blob_hashes() {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let data = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        let mut hasher = DefaultHasher::new();
        data.hash(&mut hasher);
        let before = hasher.finish();

        let path = "projects/p1/files/image_a.png";
        let export = export_with(vec![asset(path, "image/png", data.len() as u64)]);
        let blob = Blob {
            path: path.to_string(),
            mime_type: "image/png".to_string(),
            data,
        };
        let zip = pack_export(&export, std::slice::from_ref(&blob)).unwrap();
        let (back, blobs) = unpack_export(&zip).unwrap();
        assert_eq!(back, export);
        assert_eq!(blobs.len(), 1);
        assert_eq!(blobs[0].path, path);
        assert_eq!(blobs[0].mime_type, "image/png");
        let mut hasher = DefaultHasher::new();
        blobs[0].data.hash(&mut hasher);
        assert_eq!(hasher.finish(), before);
        assert_eq!(blobs[0].data, blob.data);
    }

    #[test]
    fn packed_archive_is_deterministic() {
        let export = export_with(Vec::new());
        let first = pack_export(&export, &[]).unwrap();
        let second = pack_export(&export, &[]).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn pack_rejects_missing_unreferenced_and_mismatched_blobs() {
        let path = "projects/p1/files/a.bin";
        let export = export_with(vec![asset(path, "application/octet-stream", 3)]);
        assert!(matches!(
            pack_export(&export, &[]),
            Err(ArchiveError::MissingBlob { .. })
        ));

        let blob = Blob {
            path: path.to_string(),
            mime_type: "application/octet-stream".to_string(),
            data: vec![1, 2, 3],
        };
        let extra = Blob {
            path: "projects/p1/files/extra.bin".to_string(),
            mime_type: "application/octet-stream".to_string(),
            data: vec![9],
        };
        assert!(matches!(
            pack_export(&export, &[blob.clone(), extra]),
            Err(ArchiveError::UnreferencedBlob { .. })
        ));

        assert!(matches!(
            pack_export(
                &export,
                &[Blob {
                    data: vec![1, 2],
                    ..blob.clone()
                }]
            ),
            Err(ArchiveError::SizeMismatch { .. })
        ));
        assert!(matches!(
            pack_export(
                &export,
                &[Blob {
                    mime_type: "image/png".to_string(),
                    ..blob
                }]
            ),
            Err(ArchiveError::MimeMismatch { .. })
        ));
    }

    #[test]
    fn unpack_rejects_missing_manifest_and_truncation() {
        assert!(matches!(unpack_export(&[]), Err(ArchiveError::Truncated)));

        let raw = raw_zip(&[("not-manifest.txt", b"x")], None);
        assert!(matches!(
            unpack_export(&raw),
            Err(ArchiveError::MissingManifest)
        ));

        let packed = pack_export(&export_with(Vec::new()), &[]).unwrap();
        let truncated = &packed[..packed.len() / 2];
        assert!(matches!(
            unpack_export(truncated),
            Err(ArchiveError::Truncated)
        ));
    }

    #[test]
    fn unpack_rejects_traversal_and_duplicate_member_names() {
        let raw = raw_zip(&[(PROJECTS_MANIFEST, b"{}"), ("../escape.txt", b"x")], None);
        assert!(matches!(
            unpack_export(&raw),
            Err(ArchiveError::UnsafeMemberPath { .. })
        ));

        let raw = raw_zip(
            &[(PROJECTS_MANIFEST, b"{}"), (PROJECTS_MANIFEST, b"{}")],
            None,
        );
        assert!(matches!(
            unpack_export(&raw),
            Err(ArchiveError::DuplicateMember { .. })
        ));
    }

    #[test]
    fn unpack_rejects_bad_crc_and_oversized_entries() {
        let raw = raw_zip(&[(PROJECTS_MANIFEST, b"{}")], Some(0));
        assert!(matches!(
            unpack_export(&raw),
            Err(ArchiveError::CrcMismatch { .. })
        ));

        let raw = raw_zip(&[(PROJECTS_MANIFEST, b"{}")], None);
        let limits = ArchiveLimits {
            max_entries: 16,
            max_entry_bytes: 1,
            max_total_bytes: 1024,
        };
        assert!(matches!(
            unpack_export_with_limits(&raw, &limits),
            Err(ArchiveError::LimitExceeded { .. })
        ));
    }

    #[test]
    fn unpack_rejects_manifest_mismatches() {
        let path = "projects/p1/files/a.bin";
        let export = export_with(vec![asset(path, "application/octet-stream", 3)]);
        let manifest = serde_json::to_vec_pretty(&export).unwrap();
        let raw = raw_zip(&[(PROJECTS_MANIFEST, manifest.as_slice())], None);
        assert!(matches!(
            unpack_export(&raw),
            Err(ArchiveError::MissingBlob { .. })
        ));

        let raw = raw_zip(
            &[(PROJECTS_MANIFEST, manifest.as_slice()), (path, b"abcd")],
            None,
        );
        assert!(matches!(
            unpack_export(&raw),
            Err(ArchiveError::SizeMismatch { .. })
        ));
    }

    /// Build a store-only ZIP without validation so malformed archives can be
    /// exercised (`bad_crc` overrides every member checksum when set).
    fn raw_zip(entries: &[(&str, &[u8])], bad_crc: Option<u32>) -> Vec<u8> {
        use crate::archive_wire::{
            push_u16, push_u32, CENTRAL_HEADER_LEN, CENTRAL_SIGNATURE, DOS_DATE, DOS_TIME,
            END_RECORD_LEN, FLAG_UTF8, LOCAL_HEADER_LEN, LOCAL_SIGNATURE, VERSION,
        };
        let mut out = Vec::new();
        let mut central = Vec::new();
        for (name, data) in entries {
            let name_bytes = name.as_bytes();
            let crc = bad_crc.unwrap_or_else(|| crate::archive_wire::crc32(data));
            let offset = out.len() as u32;
            push_u32(&mut out, LOCAL_SIGNATURE);
            push_u16(&mut out, VERSION);
            push_u16(&mut out, FLAG_UTF8);
            push_u16(&mut out, 0);
            push_u16(&mut out, DOS_TIME);
            push_u16(&mut out, DOS_DATE);
            push_u32(&mut out, crc);
            push_u32(&mut out, data.len() as u32);
            push_u32(&mut out, data.len() as u32);
            push_u16(&mut out, name_bytes.len() as u16);
            push_u16(&mut out, 0);
            out.extend_from_slice(name_bytes);
            out.extend_from_slice(data);

            push_u32(&mut central, CENTRAL_SIGNATURE);
            push_u16(&mut central, VERSION);
            push_u16(&mut central, VERSION);
            push_u16(&mut central, FLAG_UTF8);
            push_u16(&mut central, 0);
            push_u16(&mut central, DOS_TIME);
            push_u16(&mut central, DOS_DATE);
            push_u32(&mut central, crc);
            push_u32(&mut central, data.len() as u32);
            push_u32(&mut central, data.len() as u32);
            push_u16(&mut central, name_bytes.len() as u16);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u16(&mut central, 0);
            push_u32(&mut central, 0);
            push_u32(&mut central, offset);
            central.extend_from_slice(name_bytes);
        }
        let cd_offset = out.len() as u32;
        let cd_size = central.len() as u32;
        out.extend_from_slice(&central);
        push_u32(&mut out, crate::archive_wire::END_SIGNATURE);
        push_u16(&mut out, 0);
        push_u16(&mut out, 0);
        push_u16(&mut out, entries.len() as u16);
        push_u16(&mut out, entries.len() as u16);
        push_u32(&mut out, cd_size);
        push_u32(&mut out, cd_offset);
        push_u16(&mut out, 0);
        debug_assert_eq!(
            out.len(),
            cd_offset as usize + cd_size as usize + END_RECORD_LEN
        );
        debug_assert_eq!(LOCAL_HEADER_LEN, 30);
        debug_assert_eq!(CENTRAL_HEADER_LEN, 46);
        out
    }
}
