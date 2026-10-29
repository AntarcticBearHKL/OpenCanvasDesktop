//! Layer list + geometry mutations — port of `web/src/lib/canvas/ps/layer-ops.ts`.
//! Every function is a pure `Vec<CanvasPsLayer> -> Vec<CanvasPsLayer>` transform;
//! the web's `commitBoardLayers` callback is replaced by returning the new list
//! (styling, undo and persistence stay the caller's concern).

use crate::board::{layer_box, layer_child_ids, top_layers};
use crate::doc::{CanvasPsLayer, CanvasPsLayerKind, CanvasPsLayerStyleType, IdGen, PsParams};
use crate::geom::{rotate_point, Bounds, Point};
use crate::layer_styles::create_ps_layer_style;
use crate::surface::Surface;
use std::collections::{HashMap, HashSet, VecDeque};

/// `PsResizeCorner` (layer-ops.ts:7).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PsResizeCorner {
    Nw,
    Ne,
    Sw,
    Se,
}

/// `PsLayerDragPosition` (layer-ops.ts drag positions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LayerPosition {
    Before,
    After,
    Into,
}

const MIN_LAYER_SIZE: f64 = 8.0;

/// `syncPsGroupBoxes` (layer-ops.ts:32).
pub fn sync_group_boxes(layers: &[CanvasPsLayer]) -> Vec<CanvasPsLayer> {
    layers
        .iter()
        .map(|layer| {
            if layer.kind != CanvasPsLayerKind::Group {
                return layer.clone();
            }
            let box_ = layer_box(layers, layer);
            let mut next = layer.clone();
            next.x = box_.x;
            next.y = box_.y;
            next.width = box_.width;
            next.height = box_.height;
            next
        })
        .collect()
}

/// `findPsLayer` (layer-ops.ts:23).
pub fn find_ps_layer<'a>(layers: &'a [CanvasPsLayer], id: &str) -> Option<&'a CanvasPsLayer> {
    layers.iter().find(|layer| layer.id == id)
}

/// `patchPsLayer` (layer-ops.ts:36).
pub fn patch_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    patch: impl FnOnce(&mut CanvasPsLayer),
) -> Vec<CanvasPsLayer> {
    let mut next = layers.to_vec();
    if let Some(index) = next.iter().position(|layer| layer.id == id) {
        if let Some(target) = next.get_mut(index) {
            patch(target);
        }
    }
    next
}

/// `addPsLayer` (layer-ops.ts:40).
pub fn add_ps_layer(
    layers: &[CanvasPsLayer],
    layer: CanvasPsLayer,
    group_id: Option<&str>,
) -> Vec<CanvasPsLayer> {
    let group = group_id.and_then(|id| find_ps_layer(layers, id));
    let mut next = layers.to_vec();
    next.push(layer.clone());
    if let Some(group) = group {
        if group.kind == CanvasPsLayerKind::Group {
            let mut children = group.children.clone().unwrap_or_default();
            children.push(layer.id.clone());
            return sync_group_boxes(&patch_ps_layer(&next, &group.id, |target| {
                target.children = Some(children);
            }));
        }
    }
    sync_group_boxes(&next)
}

/// `addPsLayerAbove` (layer-ops.ts:46).
pub fn add_ps_layer_above(
    layers: &[CanvasPsLayer],
    layer: CanvasPsLayer,
    target_id: Option<&str>,
) -> Vec<CanvasPsLayer> {
    let Some(target) = target_id.and_then(|id| find_ps_layer(layers, id)) else {
        return add_ps_layer(layers, layer, None);
    };
    if let Some(parent_id) = child_parent_map(layers).get(&target.id).cloned() {
        let mut children = find_ps_layer(layers, &parent_id)
            .and_then(|item| item.children.clone())
            .unwrap_or_default();
        let index = children
            .iter()
            .position(|id| *id == target.id)
            .map_or(children.len(), |index| index + 1);
        children.insert(index, layer.id.clone());
        let mut next = layers.to_vec();
        next.push(layer);
        return sync_group_boxes(&patch_ps_layer(&next, &parent_id, |parent| {
            parent.children = Some(children);
        }));
    }
    let index = layers
        .iter()
        .position(|item| item.id == target.id)
        .map_or(layers.len(), |index| index + 1);
    let mut next = layers.to_vec();
    next.insert(index, layer);
    sync_group_boxes(&next)
}

/// `patchPsLayerStyle` (layer-ops.ts:59).
pub fn patch_ps_layer_style(
    layers: &[CanvasPsLayer],
    layer_id: &str,
    style_id: &str,
    enabled: Option<bool>,
    params: Option<PsParams>,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, layer_id) else {
        return layers.to_vec();
    };
    if layer.styles.is_none() {
        return layers.to_vec();
    }
    patch_ps_layer(layers, layer_id, |target| {
        if let Some(styles) = target.styles.as_mut() {
            for style in styles.iter_mut() {
                if style.id == style_id {
                    if let Some(enabled) = enabled {
                        style.enabled = enabled;
                    }
                    if let Some(params) = &params {
                        for (key, value) in params {
                            style.params.insert(key.clone(), value.clone());
                        }
                    }
                }
            }
        }
    })
}

/// `ensurePsLayerStyle` (layer-ops.ts:65).
pub fn ensure_ps_layer_style(
    layers: &[CanvasPsLayer],
    layer_id: &str,
    style_type: CanvasPsLayerStyleType,
    style_id: &str,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, layer_id) else {
        return layers.to_vec();
    };
    if layer.kind == CanvasPsLayerKind::Adjustment
        || layer
            .styles
            .as_ref()
            .is_some_and(|styles| styles.iter().any(|style| style.style_type == style_type))
    {
        return layers.to_vec();
    }
    let style = create_ps_layer_style(style_type, style_id);
    patch_ps_layer(layers, layer_id, |target| {
        let mut styles = target.styles.clone().unwrap_or_default();
        styles.push(style);
        target.styles = Some(styles);
    })
}

/// `rasterizePsLayer` (layer-ops.ts:72): bake a layer into a pixel layer.
pub fn rasterize_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    storage_key: &str,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id).cloned() else {
        return layers.to_vec();
    };
    let children: HashSet<String> = layer
        .children
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    let next: Vec<CanvasPsLayer> = layers
        .iter()
        .filter(|item| !children.contains(&item.id))
        .map(|item| {
            if item.id != id {
                return item.clone();
            }
            let mut baked = item.clone();
            baked.kind = CanvasPsLayerKind::Pixel;
            baked.storage_key = Some(storage_key.to_string());
            baked.content = None;
            baked.children = None;
            baked.adjustment = None;
            baked
        })
        .collect();
    sync_group_boxes(&next)
}

/// `removePsLayer` (layer-ops.ts:83).
pub fn remove_ps_layer(layers: &[CanvasPsLayer], id: &str) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id) else {
        return layers.to_vec();
    };
    let mut removed: HashSet<String> = layer
        .children
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    removed.insert(id.to_string());
    let next: Vec<CanvasPsLayer> = layers
        .iter()
        .filter(|item| !removed.contains(&item.id))
        .map(|item| {
            let mut next = item.clone();
            if let Some(children) = &item.children {
                next.children = Some(
                    children
                        .iter()
                        .filter(|child| !removed.contains(*child))
                        .cloned()
                        .collect(),
                );
            }
            next
        })
        .collect();
    sync_group_boxes(&next)
}

/// `duplicatePsLayer` (layer-ops.ts:90).
pub fn duplicate_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    suffix: &str,
    copy_id: &str,
    gen: &mut IdGen,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id).cloned() else {
        return layers.to_vec();
    };
    let child_copies: Vec<CanvasPsLayer> = layer
        .children
        .clone()
        .unwrap_or_default()
        .iter()
        .filter_map(|child_id| find_ps_layer(layers, child_id))
        .map(|child| {
            let mut copy = child.clone();
            copy.id = gen.next_id();
            copy.name = format!("{} {suffix}", child.name);
            copy
        })
        .collect();
    let mut copy = layer.clone();
    copy.id = copy_id.to_string();
    copy.name = format!("{} {suffix}", layer.name);
    copy.children = layer
        .children
        .as_ref()
        .map(|_| child_copies.iter().map(|child| child.id.clone()).collect());
    let index = layers.iter().position(|item| item.id == id).unwrap_or(0);
    let mut next: Vec<CanvasPsLayer> = layers.to_vec();
    next.insert(index + 1, copy);
    for child in child_copies {
        next.push(child);
    }
    sync_group_boxes(&next)
}

/// `groupPsLayers` (layer-ops.ts:103): a single level, never nested.
pub fn group_ps_layers(
    layers: &[CanvasPsLayer],
    ids: &[String],
    group_id: &str,
    name: &str,
) -> Vec<CanvasPsLayer> {
    let child_ids = layer_child_ids(layers);
    let selected: Vec<CanvasPsLayer> = ids
        .iter()
        .filter_map(|id| find_ps_layer(layers, id))
        .filter(|layer| layer.kind != CanvasPsLayerKind::Group && !child_ids.contains(&layer.id))
        .cloned()
        .collect();
    if selected.is_empty() {
        return layers.to_vec();
    }
    let boxes: Vec<Bounds> = selected
        .iter()
        .map(|layer| Bounds::new(layer.x, layer.y, layer.width, layer.height))
        .collect();
    let Some(box_) = Bounds::union(&boxes) else {
        return layers.to_vec();
    };
    let index = selected
        .iter()
        .filter_map(|layer| layers.iter().position(|item| item.id == layer.id))
        .max()
        .unwrap_or(0);
    let mut group =
        crate::board::base_layer(group_id.to_string(), name, CanvasPsLayerKind::Group, box_);
    group.children = Some(selected.iter().map(|layer| layer.id.clone()).collect());
    let mut next: Vec<CanvasPsLayer> = layers.to_vec();
    next.insert(index + 1, group);
    sync_group_boxes(&next)
}

/// `ungroupPsLayer` (layer-ops.ts:116).
pub fn ungroup_ps_layer(layers: &[CanvasPsLayer], id: &str) -> Vec<CanvasPsLayer> {
    match find_ps_layer(layers, id) {
        Some(group) if group.kind == CanvasPsLayerKind::Group => {
            let next: Vec<CanvasPsLayer> = layers
                .iter()
                .filter(|layer| layer.id != id)
                .cloned()
                .collect();
            sync_group_boxes(&next)
        }
        _ => layers.to_vec(),
    }
}

fn child_parent_map(layers: &[CanvasPsLayer]) -> HashMap<String, String> {
    let mut map = HashMap::new();
    for layer in layers {
        if let Some(children) = &layer.children {
            for child in children {
                map.insert(child.clone(), layer.id.clone());
            }
        }
    }
    map
}

fn reorder_top_level(layers: &[CanvasPsLayer], order: &[String]) -> Vec<CanvasPsLayer> {
    let by_id: HashMap<&str, &CanvasPsLayer> = layers
        .iter()
        .map(|layer| (layer.id.as_str(), layer))
        .collect();
    let top_level: HashSet<String> = top_layers(layers)
        .iter()
        .map(|layer| layer.id.clone())
        .collect();
    let mut queue: VecDeque<&String> = order.iter().collect();
    layers
        .iter()
        .map(|layer| {
            if top_level.contains(&layer.id) {
                queue
                    .pop_front()
                    .and_then(|id| by_id.get(id.as_str()))
                    .map_or_else(|| layer.clone(), |found| (*found).clone())
            } else {
                layer.clone()
            }
        })
        .collect()
}

/// `movePsLayerTo` (layer-ops.ts:135).
pub fn move_ps_layer_to(
    layers: &[CanvasPsLayer],
    drag_id: &str,
    target_id: &str,
    position: LayerPosition,
) -> Vec<CanvasPsLayer> {
    let Some(drag) = find_ps_layer(layers, drag_id) else {
        return layers.to_vec();
    };
    let Some(target) = find_ps_layer(layers, target_id) else {
        return layers.to_vec();
    };
    if drag_id == target_id {
        return layers.to_vec();
    }
    let parent_of_target = child_parent_map(layers).get(target_id).cloned();
    let parent_id = parent_of_target.or_else(|| {
        (target.kind == CanvasPsLayerKind::Group && position == LayerPosition::Into)
            .then(|| target_id.to_string())
    });
    if drag.kind == CanvasPsLayerKind::Group && parent_id.is_some() {
        return layers.to_vec();
    }
    let detached: Vec<CanvasPsLayer> = layers
        .iter()
        .map(|layer| {
            if layer
                .children
                .as_ref()
                .is_some_and(|children| children.contains(&drag_id.to_string()))
            {
                let mut next = layer.clone();
                next.children = Some(
                    layer
                        .children
                        .clone()
                        .unwrap_or_default()
                        .into_iter()
                        .filter(|child| child != drag_id)
                        .collect(),
                );
                next
            } else {
                layer.clone()
            }
        })
        .collect();
    let parent = parent_id
        .as_deref()
        .and_then(|id| find_ps_layer(&detached, id));
    let container: Vec<String> = match parent {
        Some(parent) => parent.children.clone().unwrap_or_default(),
        None => top_layers(&detached)
            .iter()
            .map(|layer| layer.id.clone())
            .collect(),
    };
    let container: Vec<String> = container.into_iter().filter(|id| id != drag_id).collect();
    let index = container.iter().position(|id| id == target_id);
    let insert_at = match (index, position) {
        (Some(index), LayerPosition::After) => index + 1,
        (Some(index), LayerPosition::Before | LayerPosition::Into) => index,
        // Dropping onto the group itself appends inside it (the target id is
        // never one of its own children, so the web no-ops here).
        (None, LayerPosition::Into) if parent_id.is_some() => container.len(),
        (None, _) => return layers.to_vec(),
    };
    let mut next = container;
    next.insert(insert_at, drag_id.to_string());
    match parent_id {
        Some(parent_id) => sync_group_boxes(&patch_ps_layer(&detached, &parent_id, |item| {
            item.children = Some(next);
        })),
        None => sync_group_boxes(&reorder_top_level(&detached, &next)),
    }
}

/// `movePsLayerStep` (layer-ops.ts:151).
pub fn move_ps_layer_step(layers: &[CanvasPsLayer], id: &str, forward: bool) -> Vec<CanvasPsLayer> {
    let parent_id = child_parent_map(layers).get(id).cloned();
    let parent = parent_id
        .as_deref()
        .and_then(|pid| find_ps_layer(layers, pid));
    let ids: Vec<String> = match parent {
        Some(parent) => parent.children.clone().unwrap_or_default(),
        None => top_layers(layers)
            .iter()
            .map(|layer| layer.id.clone())
            .collect(),
    };
    let Some(index) = ids.iter().position(|item| item == id) else {
        return layers.to_vec();
    };
    let target = if forward {
        index as i64 + 1
    } else {
        index as i64 - 1
    };
    if target < 0 || target as usize >= ids.len() {
        return layers.to_vec();
    }
    let position = if forward {
        LayerPosition::After
    } else {
        LayerPosition::Before
    };
    move_ps_layer_to(layers, id, &ids[target as usize], position)
}

fn transform_targets(layer: &CanvasPsLayer) -> HashSet<String> {
    let mut targets: HashSet<String> = layer
        .children
        .clone()
        .unwrap_or_default()
        .into_iter()
        .collect();
    targets.insert(layer.id.clone());
    targets
}

/// `translatePsLayer` (layer-ops.ts:165).
pub fn translate_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    dx: f64,
    dy: f64,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id) else {
        return layers.to_vec();
    };
    let targets = transform_targets(layer);
    layers
        .iter()
        .map(|item| {
            if !targets.contains(&item.id) {
                return item.clone();
            }
            let mut next = item.clone();
            next.x += dx;
            next.y += dy;
            next
        })
        .collect()
}

/// `rotatePsLayer` (layer-ops.ts:172).
pub fn rotate_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    delta_degrees: f64,
    centre: Point,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id) else {
        return layers.to_vec();
    };
    let targets = transform_targets(layer);
    layers
        .iter()
        .map(|item| {
            if !targets.contains(&item.id) {
                return item.clone();
            }
            let next = rotate_point(
                item.x + item.width / 2.0,
                item.y + item.height / 2.0,
                centre,
                delta_degrees,
            );
            let mut out = item.clone();
            out.x = next.x - item.width / 2.0;
            out.y = next.y - item.height / 2.0;
            out.rotation += delta_degrees;
            out
        })
        .collect()
}

/// `scalePsLayer` (layer-ops.ts:183).
pub fn scale_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    sx: f64,
    sy: f64,
    anchor: Point,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id) else {
        return layers.to_vec();
    };
    let targets = transform_targets(layer);
    layers
        .iter()
        .map(|item| {
            if !targets.contains(&item.id) {
                return item.clone();
            }
            let mut next = item.clone();
            next.x = anchor.x + (item.x - anchor.x) * sx;
            next.y = anchor.y + (item.y - anchor.y) * sy;
            next.width = (item.width * sx).max(1.0);
            next.height = (item.height * sy).max(1.0);
            next.font_size = item
                .font_size
                .map(|size| (size * ((sx + sy) / 2.0)).max(1.0).round());
            next
        })
        .collect()
}

fn resize_box(box_: Bounds, pointer: Point, corner: PsResizeCorner) -> Bounds {
    let (ax, ay) = match corner {
        PsResizeCorner::Nw => (1, 1),
        PsResizeCorner::Ne => (0, 1),
        PsResizeCorner::Sw => (1, 0),
        PsResizeCorner::Se => (0, 0),
    };
    let width = if ax == 0 {
        (pointer.x - box_.x).max(MIN_LAYER_SIZE)
    } else {
        (box_.x + box_.width - pointer.x).max(MIN_LAYER_SIZE)
    };
    let height = if ay == 0 {
        (pointer.y - box_.y).max(MIN_LAYER_SIZE)
    } else {
        (box_.y + box_.height - pointer.y).max(MIN_LAYER_SIZE)
    };
    Bounds::new(
        if ax == 0 {
            box_.x
        } else {
            box_.x + box_.width - width
        },
        if ay == 0 {
            box_.y
        } else {
            box_.y + box_.height - height
        },
        width,
        height,
    )
}

/// `resizePsLayer` (layer-ops.ts:200).
pub fn resize_ps_layer(
    layers: &[CanvasPsLayer],
    id: &str,
    pointer: Point,
    corner: PsResizeCorner,
) -> Vec<CanvasPsLayer> {
    let Some(layer) = find_ps_layer(layers, id).cloned() else {
        return layers.to_vec();
    };
    let box_ = layer_box(layers, &layer);
    let (ax, ay) = match corner {
        PsResizeCorner::Nw => (1, 1),
        PsResizeCorner::Ne => (0, 1),
        PsResizeCorner::Sw => (1, 0),
        PsResizeCorner::Se => (0, 0),
    };
    if layer.kind == CanvasPsLayerKind::Group {
        let resized = resize_box(box_, pointer, corner);
        let fixed = Point::new(
            if ax == 0 { box_.x } else { box_.x + box_.width },
            if ay == 0 {
                box_.y
            } else {
                box_.y + box_.height
            },
        );
        return scale_ps_layer(
            layers,
            id,
            resized.width / box_.width.max(1.0),
            resized.height / box_.height.max(1.0),
            fixed,
        );
    }
    let centre = Point::new(layer.x + layer.width / 2.0, layer.y + layer.height / 2.0);
    let local = rotate_point(pointer.x, pointer.y, centre, -layer.rotation);
    let resized = resize_box(
        Bounds::new(layer.x, layer.y, layer.width, layer.height),
        local,
        corner,
    );
    let corner_offset = Point::new(
        if ax == 0 { 0.0 } else { resized.width },
        if ay == 0 { 0.0 } else { resized.height },
    );
    let fixed_doc = rotate_point(
        if ax == 0 {
            layer.x
        } else {
            layer.x + layer.width
        },
        if ay == 0 {
            layer.y
        } else {
            layer.y + layer.height
        },
        centre,
        layer.rotation,
    );
    let offset = rotate_point(
        corner_offset.x,
        corner_offset.y,
        Point::new(0.0, 0.0),
        layer.rotation,
    );
    let scale =
        (resized.width / layer.width.max(1.0) + resized.height / layer.height.max(1.0)) / 2.0;
    patch_ps_layer(layers, id, |target| {
        target.x = fixed_doc.x - offset.x;
        target.y = fixed_doc.y - offset.y;
        target.width = resized.width;
        target.height = resized.height;
        if target.kind == CanvasPsLayerKind::Text {
            target.font_size = layer.font_size.map(|size| (size * scale).max(1.0).round());
        }
    })
}

/// `psBakeMask` (image-studio.tsx): the layer content with the mask's alpha
/// multiplied in, ready to replace the layer's storage bitmap.
pub fn ps_bake_mask(content: &Surface, mask: &Surface) -> Surface {
    let mut out = content.clone();
    if mask.width() == content.width() && mask.height() == content.height() {
        crate::compositor::multiply_alpha(&mut out, mask);
    } else {
        let sized = mask.resample_bilinear(content.width(), content.height());
        crate::compositor::multiply_alpha(&mut out, &sized);
    }
    out
}

#[cfg(test)]
mod tests {

    use super::*;

    fn pixel(gen: &mut IdGen, name: &str) -> CanvasPsLayer {
        crate::board::create_ps_pixel_layer(gen, (100.0, 100.0), name)
    }

    #[test]
    fn add_patch_remove_round_trip() {
        let mut gen = IdGen::new("l");
        let a = pixel(&mut gen, "a");
        let layers = add_ps_layer(&[], a.clone(), None);
        assert_eq!(layers.len(), 1);
        let patched = patch_ps_layer(&layers, &a.id, |layer| layer.opacity = 0.5);
        assert!((patched[0].opacity - 0.5).abs() < 1e-9);
        let removed = remove_ps_layer(&patched, &a.id);
        assert!(removed.is_empty());
    }

    #[test]
    fn bake_mask_multiplies_alpha_and_resamples() {
        let mut content = Surface::new(2, 2);
        content.fill([10, 20, 30, 200]);
        let mut mask = Surface::new(2, 2);
        mask.fill([255, 255, 255, 128]);
        let baked = ps_bake_mask(&content, &mask);
        assert_eq!(baked.get(0, 0), [10, 20, 30, 100]);

        let small = Surface::new(1, 1);
        let baked = ps_bake_mask(&content, &small);
        assert_eq!(baked.get(0, 0)[3], 0, "a cleared mask resamples to zero");
        assert_eq!((baked.width(), baked.height()), (2, 2));
    }

    #[test]
    fn group_and_ungroup() {
        let mut gen = IdGen::new("l");
        let a = pixel(&mut gen, "a");
        let b = pixel(&mut gen, "b");
        let layers = vec![a.clone(), b.clone()];
        let grouped = group_ps_layers(&layers, &[a.id.clone(), b.id.clone()], "g", "Group");
        assert_eq!(grouped.len(), 3);
        let group = grouped.iter().find(|layer| layer.id == "g").expect("group");
        assert_eq!(group.children.as_ref().map(Vec::len), Some(2));
        let ungrouped = ungroup_ps_layer(&grouped, "g");
        assert_eq!(ungrouped.len(), 2);
    }

    #[test]
    fn reorder_before_after_into() {
        let mut gen = IdGen::new("l");
        let a = pixel(&mut gen, "a");
        let b = pixel(&mut gen, "b");
        let c = pixel(&mut gen, "c");
        let layers = vec![a.clone(), b.clone(), c.clone()];
        let moved = move_ps_layer_to(&layers, &a.id, &c.id, LayerPosition::After);
        let order: Vec<&str> = moved.iter().map(|layer| layer.id.as_str()).collect();
        assert_eq!(order, vec![b.id.as_str(), c.id.as_str(), a.id.as_str()]);
    }

    #[test]
    fn reorder_into_appends_inside_the_group() {
        let mut gen = IdGen::new("l");
        let a = pixel(&mut gen, "a");
        let b = pixel(&mut gen, "b");
        let c = pixel(&mut gen, "c");
        let grouped = group_ps_layers(
            &[a.clone(), b.clone(), c.clone()],
            std::slice::from_ref(&a.id),
            "g",
            "Group",
        );
        let moved = move_ps_layer_to(&grouped, &c.id, "g", LayerPosition::Into);
        let group = moved.iter().find(|layer| layer.id == "g").expect("group");
        assert_eq!(
            group.children.as_deref(),
            Some(&[a.id.clone(), c.id.clone()][..])
        );

        let moved = move_ps_layer_to(&grouped, &c.id, "g", LayerPosition::Before);
        let group = moved.iter().find(|layer| layer.id == "g").expect("group");
        assert_eq!(
            group.children.as_deref(),
            Some(&[a.id.clone()][..]),
            "before needs a real target"
        );
    }

    #[test]
    fn transform_ops_move_geometry() {
        let mut gen = IdGen::new("l");
        let a = pixel(&mut gen, "a");
        let layers = vec![a.clone()];
        let moved = translate_ps_layer(&layers, &a.id, 5.0, 7.0);
        assert!((moved[0].x - 5.0).abs() < 1e-9);
        let scaled = scale_ps_layer(&layers, &a.id, 2.0, 2.0, Point::new(0.0, 0.0));
        assert!((scaled[0].width - 200.0).abs() < 1e-9);
        let rotated = rotate_ps_layer(&layers, &a.id, 90.0, Point::new(50.0, 50.0));
        assert!((rotated[0].rotation - 90.0).abs() < 1e-9);
    }

    #[test]
    fn duplicate_copies_children_and_ids() {
        let mut gen = IdGen::new("l");
        let mut group = pixel(&mut gen, "g");
        let child = pixel(&mut gen, "c");
        group.kind = CanvasPsLayerKind::Group;
        group.children = Some(vec![child.id.clone()]);
        let layers = vec![group, child];
        let duplicated = duplicate_ps_layer(&layers, "l-1", "copy", "copy-1", &mut gen);
        assert!(duplicated.iter().any(|layer| layer.id == "copy-1"));
        let copy = duplicated
            .iter()
            .find(|layer| layer.id == "copy-1")
            .expect("copy");
        assert_eq!(copy.children.as_ref().map(Vec::len), Some(1));
    }
}
