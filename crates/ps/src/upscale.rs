//! AI-upscale pipeline plumbing — the local size math from
//! `web/src/lib/canvas/canvas-image-data.ts` plus a tiling/stitch seam around an
//! AI-enhanced result. The model call itself lives in `open-canvas-ai`.

use crate::surface::Surface;

/// `MAX_UPSCALE_LONG_EDGE` (canvas-image-data.ts:12).
pub const MAX_UPSCALE_LONG_EDGE: usize = 4096;
/// `MIN_UPSCALE_LONG_EDGE` (canvas-image-data.ts:13).
pub const MIN_UPSCALE_LONG_EDGE: usize = 64;
/// Default tile edge (`plan_upscale_tiles`): keeps one AI call bounded.
pub const DEFAULT_UPSCALE_TILE: usize = 1024;
/// `canvas.editors.resolutionAiFactorPrompt` (en-US), split at `{{factor}}`.
pub const RESOLUTION_AI_FACTOR_PROMPT: &str = "Enhance the image resolution to";
/// `canvas.editors.resolutionAiDefaultPrompt` (en-US).
pub const RESOLUTION_AI_DEFAULT_PROMPT: &str = "Enhance the resolution and detail of this image, keep the subject, composition and style unchanged, and output a clearer high-resolution image.";

/// `ImageUpscaleAlgorithm` (canvas-image-data.ts:10).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpscaleAlgorithm {
    Nearest,
    Bilinear,
    High,
}

/// `resolveUpscaleSize` (canvas-image-data.ts:74): clamp the target long edge and
/// scale both axes by the same factor.
pub fn resolve_upscale_size(
    width: usize,
    height: usize,
    target_long_edge: usize,
) -> (usize, usize) {
    let long_edge = width.max(height).max(1);
    let target = target_long_edge.clamp(MIN_UPSCALE_LONG_EDGE, MAX_UPSCALE_LONG_EDGE);
    let scale = target as f64 / long_edge as f64;
    let out_width = (width as f64 * scale).round().max(1.0) as usize;
    let out_height = (height as f64 * scale).round().max(1.0) as usize;
    (out_width, out_height)
}

/// `canvas-node-resolution-dialog.tsx:165`: the enhancement prompt prefill plus
/// the user's optional addition, trimmed as the web trims it.
pub fn upscale_ai_prompt(factor: &str, custom: &str) -> String {
    format!("{RESOLUTION_AI_FACTOR_PROMPT} {factor}.\n{}", custom.trim())
        .trim()
        .to_string()
}

/// A pixel rectangle in source (`TileRect.source`) or target coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TileRect {
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}

/// One planned tile: where to crop the source and where to paste the result.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UpscaleTile {
    pub index: usize,
    pub row: usize,
    pub column: usize,
    pub source: TileRect,
    pub target: TileRect,
}

/// The full tiling for one upscale: source/target sizes plus every tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpscaleTilePlan {
    pub source_width: usize,
    pub source_height: usize,
    pub target_width: usize,
    pub target_height: usize,
    pub tiles: Vec<UpscaleTile>,
}

/// `buildSplitCuts` (canvas-image-data.ts:63): even `count` divisions of `size`,
/// `count + 1` boundary offsets inclusive of both edges.
fn even_cuts(size: usize, count: usize) -> Vec<usize> {
    let count = count.max(1);
    (0..=count).map(|index| index * size / count).collect()
}

/// Plan `max_tile`-sized chunks over the source, with each tile's target rect
/// derived from the shared upsample scale so adjacent tiles meet exactly.
pub fn plan_upscale_tiles(
    width: usize,
    height: usize,
    target_long_edge: usize,
    max_tile: usize,
) -> UpscaleTilePlan {
    let (target_width, target_height) = resolve_upscale_size(width, height, target_long_edge);
    let max_tile = max_tile.max(1);
    let columns = width.div_ceil(max_tile).max(1);
    let rows = height.div_ceil(max_tile).max(1);
    let x_cuts = even_cuts(width, columns);
    let y_cuts = even_cuts(height, rows);
    let scale_x = target_width as f64 / width.max(1) as f64;
    let scale_y = target_height as f64 / height.max(1) as f64;
    let mut tiles = Vec::with_capacity(rows.saturating_mul(columns));
    for row in 0..rows {
        for column in 0..columns {
            let sx = x_cuts[column];
            let sw = x_cuts[column + 1] - sx;
            let sy = y_cuts[row];
            let sh = y_cuts[row + 1] - sy;
            let tx = (sx as f64 * scale_x).round() as usize;
            let ty = (sy as f64 * scale_y).round() as usize;
            let tx2 = ((sx + sw) as f64 * scale_x).round() as usize;
            let ty2 = ((sy + sh) as f64 * scale_y).round() as usize;
            tiles.push(UpscaleTile {
                index: row * columns + column,
                row,
                column,
                source: TileRect {
                    x: sx,
                    y: sy,
                    width: sw,
                    height: sh,
                },
                target: TileRect {
                    x: tx,
                    y: ty,
                    width: tx2.saturating_sub(tx).max(1),
                    height: ty2.saturating_sub(ty).max(1),
                },
            });
        }
    }
    UpscaleTilePlan {
        source_width: width,
        source_height: height,
        target_width,
        target_height,
        tiles,
    }
}

/// Crop a source rectangle; out-of-range reads are transparent (`Surface::get`).
pub fn crop_surface(surface: &Surface, rect: TileRect) -> Surface {
    let mut out = Surface::new(rect.width.max(1), rect.height.max(1));
    for y in 0..out.height() {
        for x in 0..out.width() {
            let pixel = surface.get(rect.x as i64 + x as i64, rect.y as i64 + y as i64);
            out.set(x as i64, y as i64, pixel);
        }
    }
    out
}

/// `drawResize`/`drawStepUpscale` (canvas-image-data.ts:88): nearest, bilinear,
/// or the iterative doubling of the high-quality path.
pub fn resample_surface(
    surface: &Surface,
    width: usize,
    height: usize,
    algorithm: UpscaleAlgorithm,
) -> Surface {
    let width = width.max(1);
    let height = height.max(1);
    match algorithm {
        UpscaleAlgorithm::Bilinear => surface.resample_bilinear(width, height),
        UpscaleAlgorithm::Nearest => resample_nearest(surface, width, height),
        UpscaleAlgorithm::High => resample_high(surface, width, height),
    }
}

fn resample_nearest(surface: &Surface, width: usize, height: usize) -> Surface {
    let mut out = Surface::new(width, height);
    if surface.width() == 0 || surface.height() == 0 {
        return out;
    }
    for y in 0..height {
        let sy = (y * surface.height() / height).min(surface.height() - 1);
        for x in 0..width {
            let sx = (x * surface.width() / width).min(surface.width() - 1);
            out.set(x as i64, y as i64, surface.get(sx as i64, sy as i64));
        }
    }
    out
}

fn resample_high(surface: &Surface, width: usize, height: usize) -> Surface {
    let mut current = surface.clone();
    while current.width() > 0
        && current.height() > 0
        && current.width() * 2 < width
        && current.height() * 2 < height
    {
        current = current.resample_bilinear(current.width() * 2, current.height() * 2);
    }
    current.resample_bilinear(width, height)
}

/// Composite the per-tile AI (or local) results back into one target surface.
/// A missing tile stays transparent.
pub fn stitch_upscale_tiles(plan: &UpscaleTilePlan, tiles: &[Option<Surface>]) -> Option<Surface> {
    if plan.target_width == 0 || plan.target_height == 0 {
        return None;
    }
    let mut out = Surface::new(plan.target_width, plan.target_height);
    for (tile, source) in plan.tiles.iter().zip(tiles.iter()) {
        let Some(source) = source else {
            continue;
        };
        let fitted = if source.width() == tile.target.width && source.height() == tile.target.height
        {
            source.clone()
        } else {
            source.resample_bilinear(tile.target.width, tile.target.height)
        };
        blit(
            &mut out,
            &fitted,
            tile.target.x as i64,
            tile.target.y as i64,
        );
    }
    Some(out)
}

fn blit(dest: &mut Surface, source: &Surface, dx: i64, dy: i64) {
    for y in 0..source.height() {
        for x in 0..source.width() {
            dest.set(dx + x as i64, dy + y as i64, source.get(x as i64, y as i64));
        }
    }
}

#[cfg(test)]
mod tests {

    use super::*;

    fn fabric(width: usize, height: usize) -> Surface {
        let mut surface = Surface::new(width, height);
        for y in 0..height {
            for x in 0..width {
                let rgba = [
                    (x * 40 % 256) as u8,
                    (y * 40 % 256) as u8,
                    ((x + y) * 20 % 256) as u8,
                    255,
                ];
                surface.set(x as i64, y as i64, rgba);
            }
        }
        surface
    }

    #[test]
    fn resolve_size_clamps_and_scales() {
        assert_eq!(resolve_upscale_size(1024, 768, 2048), (2048, 1536));
        assert_eq!(resolve_upscale_size(100, 50, 10_000), (4096, 2048));
        assert_eq!(resolve_upscale_size(100, 50, 1), (64, 32));
        assert_eq!(resolve_upscale_size(0, 0, 2048), (1, 1));
    }

    #[test]
    fn tile_plan_covers_the_target_without_gaps() {
        let plan = plan_upscale_tiles(4, 4, 8, 2);
        assert_eq!((plan.target_width, plan.target_height), (64, 64));
        assert_eq!(plan.tiles.len(), 4);
        assert_eq!((plan.tiles[0].row, plan.tiles[0].column), (0, 0));
        assert_eq!(plan.tiles[3].index, 3);
        for tile in &plan.tiles {
            assert_eq!(tile.source.width, 2);
            assert_eq!(tile.target.width, 32);
        }
        let wide = plan_upscale_tiles(1000, 1000, 2048, 256);
        assert_eq!(wide.tiles.len(), 16);
        assert_eq!((wide.target_width, wide.target_height), (2048, 2048));
    }

    #[test]
    fn identity_round_trip_restores_the_source() {
        let source = fabric(64, 64);
        let plan = plan_upscale_tiles(64, 64, 64, 32);
        assert_eq!((plan.target_width, plan.target_height), (64, 64));
        let tiles: Vec<Option<Surface>> = plan
            .tiles
            .iter()
            .map(|tile| {
                let cropped = crop_surface(&source, tile.source);
                Some(resample_surface(
                    &cropped,
                    tile.target.width,
                    tile.target.height,
                    UpscaleAlgorithm::Bilinear,
                ))
            })
            .collect();
        let stitched = stitch_upscale_tiles(&plan, &tiles).expect("stitch");
        assert_eq!(stitched, source);
    }

    #[test]
    fn scaled_stitch_has_no_transparent_gaps() {
        let mut source = Surface::new(4, 4);
        source.fill([10, 20, 30, 255]);
        let plan = plan_upscale_tiles(4, 4, 8, 2);
        assert_eq!((plan.target_width, plan.target_height), (64, 64));
        let tiles: Vec<Option<Surface>> = plan
            .tiles
            .iter()
            .map(|tile| {
                Some(resample_surface(
                    &crop_surface(&source, tile.source),
                    tile.target.width,
                    tile.target.height,
                    UpscaleAlgorithm::Bilinear,
                ))
            })
            .collect();
        let stitched = stitch_upscale_tiles(&plan, &tiles).expect("stitch");
        assert_eq!((stitched.width(), stitched.height()), (64, 64));
        for y in 0..stitched.height() {
            for x in 0..stitched.width() {
                assert_eq!(stitched.get(x as i64, y as i64), [10, 20, 30, 255]);
            }
        }
    }

    #[test]
    fn missing_tile_stays_transparent() {
        let plan = plan_upscale_tiles(4, 4, 8, 2);
        let tiles: Vec<Option<Surface>> = plan
            .tiles
            .iter()
            .map(|tile| {
                let mut surface = Surface::new(tile.target.width, tile.target.height);
                surface.fill([0, 0, 0, 255]);
                (tile.index != 3).then_some(surface)
            })
            .collect();
        let stitched = stitch_upscale_tiles(&plan, &tiles).expect("stitch");
        assert_eq!(stitched.get(0, 0)[3], 255);
        assert_eq!(stitched.get(63, 63)[3], 0);
    }

    #[test]
    fn crop_out_of_range_reads_transparent() {
        let source = fabric(4, 4);
        let cropped = crop_surface(
            &source,
            TileRect {
                x: 3,
                y: 3,
                width: 2,
                height: 2,
            },
        );
        assert_eq!(cropped.get(0, 0), source.get(3, 3));
        assert_eq!(cropped.get(1, 1), [0, 0, 0, 0]);
    }

    #[test]
    fn nearest_and_high_upscale_resize() {
        let source = fabric(2, 2);
        let nearest = resample_surface(&source, 4, 4, UpscaleAlgorithm::Nearest);
        assert_eq!((nearest.width(), nearest.height()), (4, 4));
        assert_eq!(nearest.get(0, 0), source.get(0, 0));
        let high = resample_surface(&source, 16, 16, UpscaleAlgorithm::High);
        assert_eq!((high.width(), high.height()), (16, 16));
    }

    #[test]
    fn ai_prompt_matches_the_dialog_prefill() {
        assert_eq!(
            upscale_ai_prompt("2x", "  keep it  "),
            "Enhance the image resolution to 2x.\nkeep it"
        );
        assert_eq!(
            upscale_ai_prompt("4x", "   "),
            "Enhance the image resolution to 4x."
        );
    }
}
