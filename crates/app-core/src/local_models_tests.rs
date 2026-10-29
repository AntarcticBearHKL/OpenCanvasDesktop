//! Local-model scan tests: honest installed/missing state, byte sums, and the
//! runtime flag that keeps the tab from faking a download.

use std::error::Error;
use std::fs;

use super::*;

#[test]
fn missing_files_report_not_installed() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    let view = scan(dir.path());
    assert!(
        !view.runtime_available,
        "no inference runtime in this build"
    );
    assert_eq!(view.models.len(), LOCAL_MODELS.len());
    assert!(view.models.iter().all(|model| !model.installed));
    assert!(view.models.iter().all(|model| model.bytes == 0));
    Ok(())
}

#[test]
fn fully_present_files_report_installed_with_the_byte_sum() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    fs::write(dir.path().join("isnet.onnx"), vec![0_u8; 100])?;
    fs::write(dir.path().join("mobilesam.encoder.onnx"), vec![0_u8; 40])?;
    fs::write(dir.path().join("mobilesam.decoder.onnx"), vec![0_u8; 60])?;
    let view = scan(dir.path());
    let bg = view
        .models
        .iter()
        .find(|model| model.id == "background-removal")
        .ok_or("bg descriptor")?;
    assert!(bg.installed);
    assert_eq!(bg.bytes, 100);
    let sam = view
        .models
        .iter()
        .find(|model| model.id == "mobile-sam")
        .ok_or("sam descriptor")?;
    assert!(sam.installed);
    assert_eq!(sam.bytes, 100);
    Ok(())
}

#[test]
fn a_partial_mobile_sam_download_stays_not_installed() -> Result<(), Box<dyn Error>> {
    let dir = tempfile::TempDir::new()?;
    fs::write(dir.path().join("mobilesam.encoder.onnx"), vec![0_u8; 40])?;
    let view = scan(dir.path());
    let sam = view
        .models
        .iter()
        .find(|model| model.id == "mobile-sam")
        .ok_or("sam descriptor")?;
    assert!(!sam.installed, "both ONNX files are required");
    assert_eq!(
        sam.bytes, 0,
        "partial files do not count as installed bytes"
    );
    Ok(())
}
