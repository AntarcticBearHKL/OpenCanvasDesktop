"""Canvas document logic for the desktop MCP.

The desktop app persists the same ``CanvasProject`` JSON the web app uses, so
the operation surface can stay a pure JSON reducer. Node defaults mirror
``crates/core/src/constants.rs`` and the grouping/alignment rules mirror
``web/src/lib/canvas/canvas-agent-ops.ts`` / ``alignment.ts``.
"""

from __future__ import annotations

import copy
import math
import secrets
import time
from datetime import datetime, timezone
from typing import Any

NODE_SPECS: dict[str, dict[str, Any]] = {
    "image": {
        "width": 340.0,
        "height": 240.0,
        "title": "Image",
        "metadata": {"content": "", "status": "idle"},
    },
    "image-stack": {
        "width": 420.0,
        "height": 320.0,
        "title": "Image Stack",
        "metadata": {"status": "idle", "images": []},
    },
    "image-attempt": {
        "width": 380.0,
        "height": 460.0,
        "title": "Image Attempt",
        "metadata": {
            "prompt": "",
            "status": "idle",
            "generationMode": "image",
            "attemptVersion": 1,
            "attemptFrozen": False,
            "attemptRuns": [],
        },
    },
    "text": {
        "width": 340.0,
        "height": 360.0,
        "title": "Text",
        "metadata": {"content": "", "status": "idle", "fontSize": 14},
    },
    "prompt": {
        "width": 340.0,
        "height": 240.0,
        "title": "Image Prompt",
        "metadata": {"prompt": "", "status": "idle"},
    },
    "video-prompt": {
        "width": 340.0,
        "height": 360.0,
        "title": "Video Prompt",
        "metadata": {"prompt": "", "status": "idle", "videoMode": "frames"},
    },
    "image-generation": {
        "width": 320.0,
        "height": 360.0,
        "title": "Image Generation",
        "metadata": {"status": "idle", "generationMode": "image"},
    },
    "video-generation": {
        "width": 320.0,
        "height": 320.0,
        "title": "Video Generation",
        "metadata": {
            "status": "idle",
            "generationMode": "video",
            "model": "minimax/hailuo-3-max",
        },
    },
    "video": {
        "width": 420.0,
        "height": 236.0,
        "title": "Video",
        "metadata": {"content": "", "status": "idle"},
    },
    "assets": {"width": 360.0, "height": 320.0, "title": "Assets", "metadata": {}},
    "smart-canvas": {
        "width": 420.0,
        "height": 300.0,
        "title": "Smart Canvas",
        "metadata": {},
    },
}

FALLBACK_SPEC: dict[str, Any] = {
    "width": 340.0,
    "height": 240.0,
    "title": "Node",
    "metadata": {},
}

BUILTIN_TYPES = tuple(NODE_SPECS)

PROMPT_TYPES = frozenset({"prompt", "video-prompt", "image-attempt"})
GENERATOR_TYPES = frozenset({"image-generation", "video-generation", "image-attempt"})
MEDIA_TYPES = frozenset({"image", "video"})
REMOVED_TYPES = frozenset(
    {"text", "prompt", "video-prompt", "image-generation", "video-generation", "video"}
)


def now_iso() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def new_id(prefix: str) -> str:
    return f"{prefix}-{int(time.time() * 1000)}-{secrets.token_hex(3)}"


def spec_for(node_type: str) -> dict[str, Any]:
    return copy.deepcopy(NODE_SPECS.get(node_type, FALLBACK_SPEC))


def _number(value: Any, fallback: float = 0.0) -> float:
    try:
        return float(value)
    except (TypeError, ValueError):
        return fallback


def _clean_metadata(metadata: dict[str, Any] | None) -> dict[str, Any]:
    if not isinstance(metadata, dict):
        return {}
    return {key: value for key, value in metadata.items() if value is not None}


def merge_metadata(base: dict[str, Any] | None, patch: dict[str, Any] | None) -> dict[str, Any]:
    """Shallow merge; explicit ``None`` deletes the key (the web reducer's rule)."""
    merged = copy.deepcopy(base) if isinstance(base, dict) else {}
    if not isinstance(patch, dict):
        return merged
    for key, value in patch.items():
        if value is None:
            merged.pop(key, None)
        else:
            merged[key] = copy.deepcopy(value)
    return merged


def next_canvas_x(nodes: list[dict[str, Any]]) -> float:
    if not nodes:
        return 0.0
    return max(_number(node.get("position", {}).get("x")) + _number(node.get("width")) for node in nodes) + 80.0


def find_node(project: dict[str, Any], node_id: str) -> dict[str, Any]:
    for node in project.get("nodes") or []:
        if node.get("id") == node_id:
            return node
    raise ValueError(f"节点不存在：{node_id}")


def _selection(project: dict[str, Any], ids: list[str]) -> list[str]:
    existing = {node.get("id") for node in project.get("nodes") or []}
    return [node_id for node_id in ids if node_id in existing]


def _drop_nodes(project: dict[str, Any], ids: set[str]) -> None:
    project["nodes"] = [node for node in project.get("nodes") or [] if node.get("id") not in ids]
    project["connections"] = [
        conn
        for conn in project.get("connections") or []
        if conn.get("fromNodeId") not in ids and conn.get("toNodeId") not in ids
    ]


def add_node(project: dict[str, Any], op: dict[str, Any]) -> str:
    node_type = str(op.get("nodeType") or op.get("typeName") or "image-attempt")
    if node_type in REMOVED_TYPES:
        raise ValueError(f"{node_type} was removed; use image-attempt or image")
    spec = spec_for(node_type)
    if op.get("id"):
        node_id = str(op["id"])
    else:
        node_id = new_id(node_type)
    if any(node.get("id") == node_id for node in project.get("nodes") or []):
        raise ValueError(f"节点 id 已存在：{node_id}")

    position = op.get("position")
    if isinstance(position, dict):
        x = _number(position.get("x"))
        y = _number(position.get("y"))
    elif op.get("x") is not None or op.get("y") is not None:
        x = _number(op.get("x"))
        y = _number(op.get("y"))
    else:
        x = next_canvas_x(project.get("nodes") or [])
        y = 0.0

    metadata = merge_metadata(spec["metadata"], _clean_metadata(op.get("metadata")))
    if node_type == "image-attempt":
        if not metadata.get("attemptVersion"):
            versions = [
                int(node.get("metadata", {}).get("attemptVersion") or 0)
                for node in project.get("nodes") or []
                if node.get("type") == "image-attempt"
            ]
            metadata["attemptVersion"] = max(versions, default=0) + 1
        if not metadata.get("attemptTitle"):
            metadata["attemptTitle"] = str(op.get("title") or "new attempt")
    node = {
        "id": node_id,
        "type": node_type,
        "title": str(op.get("title") or spec["title"]),
        "position": {"x": x, "y": y},
        "width": _number(op.get("width"), spec["width"]),
        "height": _number(op.get("height"), spec["height"]),
        "metadata": metadata,
    }
    project.setdefault("nodes", []).append(node)
    return node_id


def _update_node(project: dict[str, Any], op: dict[str, Any]) -> None:
    node_id = str(op.get("id") or "")
    node = find_node(project, node_id)
    patch = op.get("patch") if isinstance(op.get("patch"), dict) else {}
    for key in ("title", "position", "width", "height"):
        if key in patch:
            node[key] = copy.deepcopy(patch[key])
    patch_metadata = patch.get("metadata") if isinstance(patch.get("metadata"), dict) else None
    metadata = merge_metadata(node.get("metadata"), patch_metadata)
    metadata = merge_metadata(
        metadata,
        op.get("metadata") if isinstance(op.get("metadata"), dict) else None,
    )
    node["metadata"] = metadata


def _duplicate_node(project: dict[str, Any], op: dict[str, Any]) -> str:
    source = find_node(project, str(op.get("id") or ""))
    duplicate = copy.deepcopy(source)
    duplicate["id"] = new_id(str(source.get("type") or "node"))
    duplicate["title"] = f"{source.get('title') or 'Node'} Copy"
    offset_x = _number(op.get("dx"), 36.0)
    offset_y = _number(op.get("dy"), 36.0)
    duplicate["position"] = {
        "x": _number(source.get("position", {}).get("x")) + offset_x,
        "y": _number(source.get("position", {}).get("y")) + offset_y,
    }
    project.setdefault("nodes", []).append(duplicate)
    return duplicate["id"]


def _connect(project: dict[str, Any], op: dict[str, Any]) -> str | None:
    from_id = str(op.get("fromNodeId") or "")
    to_id = str(op.get("toNodeId") or "")
    if not from_id or not to_id or from_id == to_id:
        return None
    nodes = {node.get("id"): node for node in project.get("nodes") or []}
    if from_id not in nodes or to_id not in nodes:
        return None
    # Only a pure Image owns an output wire. Attempts and other cards are sinks.
    if nodes[from_id].get("type") != "image":
        return None
    for conn in project.get("connections") or []:
        if conn.get("fromNodeId") == from_id and conn.get("toNodeId") == to_id:
            return str(conn.get("id"))
    connection_id = str(op.get("id") or f"conn-{from_id}-{to_id}")
    connection = {"id": connection_id, "fromNodeId": from_id, "toNodeId": to_id}
    if op.get("relation"):
        connection["relation"] = str(op["relation"])
    both_attempts = (
        nodes[from_id].get("type") == "image-attempt"
        and nodes[to_id].get("type") == "image-attempt"
    )
    if both_attempts:
        connection.setdefault("kind", "derive")
        connection.setdefault("reason", "Derived version")
        connection.setdefault("changes", "prompt/config carried into the next attempt")
    for key in ("kind", "reason", "changes", "agentRunId"):
        if op.get(key):
            connection[key] = str(op[key])
    project.setdefault("connections", []).append(connection)
    return connection_id


def _fit_size(width: Any, height: Any, max_width: float, max_height: float) -> tuple[float, float]:
    width = max(1.0, _number(width, 1.0))
    height = max(1.0, _number(height, 1.0))
    scale = min(1.0, max_width / width, max_height / height)
    return width * scale, height * scale


def _collect_images(project: dict[str, Any], node_ids: list[str]) -> list[dict[str, Any]]:
    collected: list[dict[str, Any]] = []
    by_id = {node.get("id"): node for node in project.get("nodes") or []}
    for node_id in node_ids:
        node = by_id.get(node_id)
        if not node:
            continue
        metadata = node.get("metadata") or {}
        if node.get("type") == "image-stack":
            collected.extend(copy.deepcopy(metadata.get("images") or []))
        elif node.get("type") == "image" and metadata.get("content"):
            collected.append(
                {
                    "id": new_id("image"),
                    "status": "success",
                    "content": metadata.get("content"),
                    "storageKey": metadata.get("storageKey"),
                    "thumbnail": metadata.get("thumbnail"),
                    "thumbnailKey": metadata.get("thumbnailKey"),
                    "naturalWidth": _number(metadata.get("naturalWidth"), _number(node.get("width"), 1.0)),
                    "naturalHeight": _number(metadata.get("naturalHeight"), _number(node.get("height"), 1.0)),
                    "bytes": metadata.get("bytes") or 0,
                    "mimeType": metadata.get("mimeType") or "image/png",
                }
            )
    return [image for image in collected if image.get("content")]


def _pack_images(project: dict[str, Any], op: dict[str, Any]) -> str:
    node_ids = [str(item) for item in op.get("nodeIds") or []]
    images = _collect_images(project, node_ids)
    if not images:
        raise ValueError("选中的节点里没有可打包的图片内容")
    spec = spec_for("image-stack")
    position = op.get("position")
    if isinstance(position, dict):
        x = _number(position.get("x")) - spec["width"] / 2.0
        y = _number(position.get("y")) - spec["height"] / 2.0
    else:
        sources = [
            node
            for node in project.get("nodes") or []
            if node.get("id") in set(node_ids)
        ]
        x = next_canvas_x(project.get("nodes") or [])
        y = _number(sources[0].get("position", {}).get("y")) if sources else 0.0
    stack = {
        "id": str(op.get("id") or new_id("image-stack")),
        "type": "image-stack",
        "title": str(op.get("title") or spec["title"]),
        "position": {"x": x, "y": y},
        "width": spec["width"],
        "height": spec["height"],
        "metadata": {
            **spec["metadata"],
            "images": images,
            "count": len(images),
            "primaryImageId": images[0].get("id"),
            "status": "success",
        },
    }
    remove_sources = op.get("removeSources") is not False
    if remove_sources:
        _drop_nodes(project, set(node_ids))
    project.setdefault("nodes", []).append(stack)
    return stack["id"]


def _unpack_images(project: dict[str, Any], op: dict[str, Any]) -> list[str]:
    stack = find_node(project, str(op.get("nodeId") or ""))
    if stack.get("type") != "image-stack":
        raise ValueError("指定节点不是图片组（image-stack）")
    images = [
        image
        for image in (stack.get("metadata") or {}).get("images") or []
        if image.get("content")
    ]
    if not images:
        raise ValueError("图片组里没有可拆出的图片")
    edge = max(_number(stack.get("width")), _number(stack.get("height")))
    sizes = [
        _fit_size(image.get("naturalWidth"), image.get("naturalHeight"), edge, edge)
        for image in images
    ]
    cell_width = max(size[0] for size in sizes)
    cell_height = max(size[1] for size in sizes)
    gap = _number(op.get("gap"), 16.0)
    columns = max(1, int(_number(op.get("columns"), math.ceil(math.sqrt(len(images))))))
    start_x = _number(stack.get("position", {}).get("x")) + _number(stack.get("width")) + 96.0
    start_y = _number(stack.get("position", {}).get("y"))
    created: list[dict[str, Any]] = []
    for index, (image, size) in enumerate(zip(images, sizes, strict=True)):
        created.append(
            {
                "id": new_id("image"),
                "type": "image",
                "title": f"{stack.get('title') or 'Image Stack'} {index + 1}",
                "position": {
                    "x": start_x + (index % columns) * (cell_width + gap),
                    "y": start_y + (index // columns) * (cell_height + gap),
                },
                "width": size[0],
                "height": size[1],
                "metadata": {
                    "content": image.get("content"),
                    "storageKey": image.get("storageKey"),
                    "thumbnail": image.get("thumbnail"),
                    "thumbnailKey": image.get("thumbnailKey"),
                    "naturalWidth": image.get("naturalWidth"),
                    "naturalHeight": image.get("naturalHeight"),
                    "bytes": image.get("bytes"),
                    "mimeType": image.get("mimeType"),
                    "status": "success",
                    "prompt": (stack.get("metadata") or {}).get("prompt"),
                },
            }
        )
    if op.get("keepGroup") is not True:
        _drop_nodes(project, {str(stack.get("id"))})
    project.setdefault("nodes", []).extend(created)
    return [node["id"] for node in created]


ALIGN_AXES = frozenset(
    {"left", "center-x", "right", "top", "center-y", "bottom", "distribute-x", "distribute-y"}
)


def align_nodes(project: dict[str, Any], ids: list[str], axis: str) -> dict[str, dict[str, float]]:
    if axis not in ALIGN_AXES:
        raise ValueError(f"未知对齐方式：{axis}")
    nodes = [node for node in project.get("nodes") or [] if node.get("id") in set(ids)]
    if len(nodes) < 2:
        return {}
    left = min(_number(node.get("position", {}).get("x")) for node in nodes)
    top = min(_number(node.get("position", {}).get("y")) for node in nodes)
    right = max(
        _number(node.get("position", {}).get("x")) + _number(node.get("width")) for node in nodes
    )
    bottom = max(
        _number(node.get("position", {}).get("y")) + _number(node.get("height")) for node in nodes
    )
    positions: dict[str, dict[str, float]] = {}
    if axis in {"distribute-x", "distribute-y"}:
        if len(nodes) < 3:
            return {}
        horizontal = axis == "distribute-x"
        ordered = sorted(
            nodes,
            key=lambda node: _number(node.get("position", {}).get("x" if horizontal else "y")),
        )
        used = sum(_number(node.get("width" if horizontal else "height")) for node in ordered)
        gap = ((right - left) if horizontal else (bottom - top)) - used
        gap /= len(ordered) - 1
        cursor = left if horizontal else top
        for node in ordered:
            positions[str(node["id"])] = {
                "x": round(cursor) if horizontal else _number(node["position"].get("x")),
                "y": _number(node["position"].get("y")) if horizontal else round(cursor),
            }
            cursor += _number(node.get("width" if horizontal else "height")) + gap
    else:
        for node in nodes:
            width = _number(node.get("width"))
            height = _number(node.get("height"))
            x = _number(node.get("position", {}).get("x"))
            y = _number(node.get("position", {}).get("y"))
            if axis == "left":
                x = left
            elif axis == "center-x":
                x = left + ((right - left) - width) / 2.0
            elif axis == "right":
                x = right - width
            elif axis == "top":
                y = top
            elif axis == "center-y":
                y = top + ((bottom - top) - height) / 2.0
            elif axis == "bottom":
                y = bottom - height
            positions[str(node["id"])] = {"x": round(x), "y": round(y)}
    for node in nodes:
        position = positions.get(str(node["id"]))
        if position:
            node["position"] = position
    return positions


def _set_node_content(node: dict[str, Any], content: str) -> None:
    node_type = node.get("type")
    metadata = node.setdefault("metadata", {})
    if node_type in PROMPT_TYPES:
        metadata["prompt"] = content
    elif node_type not in GENERATOR_TYPES:
        metadata["content"] = content
        primary = metadata.get("primaryTextId")
        if primary:
            for item in metadata.get("texts") or []:
                if item.get("id") == primary:
                    item["content"] = content


def _video_slots(project: dict[str, Any], op: dict[str, Any]) -> dict[str, Any] | None:
    node = find_node(project, str(op.get("nodeId") or ""))
    if node.get("type") != "video-prompt":
        return None
    metadata = node.setdefault("metadata", {})
    slots = copy.deepcopy(metadata.get("videoSlots") or {})
    slot = op.get("slot")
    source = op.get("sourceNodeId")
    if op.get("type") == "clear_video_slot":
        if slot == "reference":
            slots["references"] = [
                item for item in slots.get("references") or [] if item != source
            ]
        elif slot == "firstFrame":
            slots.pop("firstFrame", None)
        elif slot == "lastFrame":
            slots.pop("lastFrame", None)
    elif slot == "reference" and source:
        references = list(slots.get("references") or [])
        if source not in references:
            references.append(str(source))
        slots["references"] = references
    elif slot in {"firstFrame", "lastFrame"} and source:
        slots[slot] = str(source)
    return slots


def apply_ops(project: dict[str, Any], ops: list[dict[str, Any]]) -> dict[str, Any]:
    """Apply a batch of canvas ops in order (the desktop `app_apply_ops` core)."""
    created: list[str] = []
    warnings: list[str] = []
    generation: list[dict[str, Any]] = []
    selection: list[str] = []
    selection_changed = False

    for index, op in enumerate(ops):
        if not isinstance(op, dict) or not op.get("type"):
            warnings.append(f"op {index}: 缺少 type，已跳过")
            continue
        if op.get("ns") not in (None, "canvas"):
            warnings.append(f"op {index}: 桌面端仅支持 canvas 命名空间，已跳过 {op.get('ns')}")
            continue
        op_type = str(op["type"])
        try:
            if op_type == "add_node":
                node_id = add_node(project, op)
                created.append(node_id)
                selection = [node_id]
                selection_changed = True
            elif op_type == "move_node":
                node = find_node(project, str(op.get("id") or ""))
                node["position"] = {
                    "x": _number(node.get("position", {}).get("x")) + _number(op.get("dx")),
                    "y": _number(node.get("position", {}).get("y")) + _number(op.get("dy")),
                }
            elif op_type == "update_node":
                _update_node(project, op)
            elif op_type in {"delete_node", "delete_nodes"}:
                ids = op.get("ids")
                if not isinstance(ids, list):
                    ids = [op.get("id")] if op.get("id") else []
                if op.get("nodeType") and not ids:
                    ids = [
                        node.get("id")
                        for node in project.get("nodes") or []
                        if node.get("type") == op.get("nodeType")
                    ]
                doomed = {str(item) for item in ids if item}
                _drop_nodes(project, doomed)
                if selection_changed:
                    selection = [item for item in selection if item not in doomed]
            elif op_type == "delete_connections":
                ids = {str(item) for item in (op.get("ids") or ([op["id"]] if op.get("id") else []))}
                if op.get("all"):
                    project["connections"] = []
                elif ids:
                    project["connections"] = [
                        conn for conn in project.get("connections") or [] if conn.get("id") not in ids
                    ]
            elif op_type == "connect_nodes":
                _connect(project, op)
            elif op_type == "set_viewport" and isinstance(op.get("viewport"), dict):
                viewport = op["viewport"]
                project["viewport"] = {
                    "x": _number(viewport.get("x")),
                    "y": _number(viewport.get("y")),
                    "k": max(0.05, min(5.0, _number(viewport.get("k"), 1.0))),
                }
            elif op_type == "select_nodes":
                selection = _selection(project, [str(item) for item in op.get("ids") or []])
                selection_changed = True
            elif op_type == "deselect_all":
                selection = []
                selection_changed = True
            elif op_type == "duplicate_node":
                node_id = _duplicate_node(project, op)
                created.append(node_id)
                selection = [node_id]
                selection_changed = True
            elif op_type == "move_node_layer":
                node_id = str(op.get("nodeId") or "")
                direction = op.get("direction")
                nodes = project.setdefault("nodes", [])
                position = next(
                    (i for i, node in enumerate(nodes) if node.get("id") == node_id), -1
                )
                if position >= 0:
                    target = position + (1 if direction == "up" else -1)
                    if 0 <= target < len(nodes):
                        node = nodes.pop(position)
                        nodes.insert(target, node)
            elif op_type == "toggle_node_flag":
                node = find_node(project, str(op.get("nodeId") or ""))
                metadata = node.setdefault("metadata", {})
                flag = str(op.get("flag") or "")
                metadata[flag] = not bool(metadata.get(flag))
            elif op_type == "rename_nodes":
                title = str(op.get("title") or "").strip()
                ids = [str(item) for item in op.get("ids") or []]
                if title:
                    for position, node_id in enumerate(ids):
                        node = next(
                            (item for item in project.get("nodes") or [] if item.get("id") == node_id),
                            None,
                        )
                        if node:
                            node["title"] = f"{title} {position + 1}" if len(ids) > 1 else title
            elif op_type == "set_font_size":
                node = find_node(project, str(op.get("nodeId") or ""))
                node.setdefault("metadata", {})["fontSize"] = _number(op.get("fontSize"), 14.0)
            elif op_type == "set_free_resize":
                node = find_node(project, str(op.get("nodeId") or ""))
                metadata = node.setdefault("metadata", {})
                free_resize = op.get("freeResize")
                if free_resize is None:
                    free_resize = not bool(metadata.get("freeResize"))
                metadata["freeResize"] = bool(free_resize)
                if not free_resize and node.get("type") == "image":
                    natural_width = _number(metadata.get("naturalWidth"), _number(node.get("width"), 1.0))
                    natural_height = _number(metadata.get("naturalHeight"), _number(node.get("height"), 1.0))
                    ratio = natural_width / max(1.0, natural_height)
                    height = _number(node.get("width")) / ratio
                    node["position"] = {
                        "x": _number(node.get("position", {}).get("x")),
                        "y": _number(node.get("position", {}).get("y"))
                        + (_number(node.get("height")) - height) / 2.0,
                    }
                    node["height"] = height
            elif op_type == "resize_node":
                node = find_node(project, str(op.get("nodeId") or ""))
                node["width"] = _number(op.get("width"), _number(node.get("width")))
                node["height"] = _number(op.get("height"), _number(node.get("height")))
                if isinstance(op.get("position"), dict):
                    node["position"] = {
                        "x": _number(op["position"].get("x")),
                        "y": _number(op["position"].get("y")),
                    }
            elif op_type == "set_node_content":
                node = find_node(project, str(op.get("nodeId") or ""))
                _set_node_content(node, str(op.get("content") or ""))
            elif op_type == "set_node_prompt":
                node = find_node(project, str(op.get("nodeId") or ""))
                if node.get("type") in PROMPT_TYPES:
                    node.setdefault("metadata", {})["prompt"] = str(op.get("prompt") or "")
            elif op_type == "set_node_title":
                node = find_node(project, str(op.get("nodeId") or ""))
                node["title"] = str(op.get("title") or "")
            elif op_type == "apply_node_metadata":
                node = find_node(project, str(op.get("nodeId") or ""))
                node["metadata"] = merge_metadata(node.get("metadata"), op.get("patch"))
            elif op_type == "set_batch_primary":
                node = find_node(project, str(op.get("nodeId") or ""))
                metadata = node.setdefault("metadata", {})
                item_id = op.get("itemId")
                if node.get("type") == "text":
                    for text in metadata.get("texts") or []:
                        if text.get("id") == item_id and text.get("content"):
                            metadata["content"] = text["content"]
                            metadata["primaryTextId"] = item_id
                            break
                elif node.get("type") == "image-stack":
                    metadata["primaryImageId"] = item_id
            elif op_type == "align_nodes":
                ids = [str(item) for item in op.get("ids") or selection]
                align_nodes(project, ids, str(op.get("axis") or "left"))
            elif op_type == "set_zoom":
                viewport = project.setdefault("viewport", {"x": 0.0, "y": 0.0, "k": 1.0})
                viewport["k"] = max(0.05, min(5.0, _number(op.get("scale"), 1.0)))
            elif op_type == "pack_images_into_group":
                node_id = _pack_images(project, op)
                created.append(node_id)
                selection = [node_id]
                selection_changed = True
            elif op_type == "unpack_image_group":
                ids = _unpack_images(project, op)
                created.extend(ids)
                selection = ids
                selection_changed = True
            elif op_type in {"bind_video_slot", "clear_video_slot", "switch_video_frame_slot"}:
                slots = _video_slots(project, op)
                if slots is not None:
                    find_node(project, str(op.get("nodeId") or ""))["metadata"]["videoSlots"] = slots
            elif op_type == "set_video_mode":
                node = find_node(project, str(op.get("nodeId") or ""))
                node.setdefault("metadata", {})["videoMode"] = op.get("mode")
            elif op_type == "set_asset_source":
                node = find_node(project, str(op.get("nodeId") or ""))
                node.setdefault("metadata", {})["assetSource"] = op.get("source")
            elif op_type == "run_generation":
                generation.append(
                    {
                        "nodeId": str(op.get("nodeId") or ""),
                        "mode": op.get("mode"),
                        "prompt": op.get("prompt"),
                    }
                )
            else:
                warnings.append(f"op {index}: 桌面端未支持的 op 类型 {op_type}，已跳过")
        except (KeyError, TypeError, ValueError) as error:
            warnings.append(f"op {index} ({op_type}): {error}")
    return {
        "createdIds": created,
        "selected": selection,
        "selectionChanged": selection_changed,
        "warnings": warnings,
        "generation": generation,
    }


def parse_data_url(value: str) -> tuple[str, str]:
    """Return ``(mime, base64_payload)`` for a data URL, else ``("", "")``."""
    if not value.startswith("data:") or "," not in value:
        return "", ""
    header, payload = value.split(",", 1)
    mime = header[5:].split(";", 1)[0] or "application/octet-stream"
    if ";base64" not in header:
        return "", ""
    return mime, payload


def node_content(project: dict[str, Any], node_id: str, item_id: str | None = None) -> dict[str, Any]:
    """Select the real bytes/text carried by one node, without copying metadata noise."""
    node = find_node(project, node_id)
    metadata = node.get("metadata") or {}
    node_type = node.get("type")
    if node_type == "image-stack":
        images = metadata.get("images") or []
        wanted = item_id or metadata.get("primaryImageId")
        image = next((item for item in images if item.get("id") == wanted), None)
        if image is None and images:
            image = images[0]
        if image is None:
            raise ValueError(f"图片组 {node_id} 没有图片")
        mime, payload = parse_data_url(str(image.get("content") or ""))
        return {
            "kind": "image",
            "nodeId": node_id,
            "itemId": image.get("id"),
            "filename": f"{image.get('id') or 'image'}.{mime.split('/')[-1] or 'png'}",
            "mimeType": mime or str(image.get("mimeType") or "image/png"),
            "base64": payload,
            "bytes": image.get("bytes") or 0,
            "naturalWidth": image.get("naturalWidth"),
            "naturalHeight": image.get("naturalHeight"),
        }
    if node_type in {"image", "video"}:
        mime, payload = parse_data_url(str(metadata.get("content") or ""))
        if not payload:
            raise ValueError(f"节点 {node_id} 没有内嵌的 {node_type} 内容")
        return {
            "kind": "image" if node_type == "image" else "video",
            "nodeId": node_id,
            "filename": f"{node_id}.{mime.split('/')[-1] or 'bin'}",
            "mimeType": mime or str(metadata.get("mimeType") or "application/octet-stream"),
            "base64": payload,
            "bytes": metadata.get("bytes") or 0,
        }
    if node_type in PROMPT_TYPES:
        return {
            "kind": "text",
            "nodeId": node_id,
            "text": str(metadata.get("prompt") or ""),
        }
    if node_type == "text":
        return {
            "kind": "text",
            "nodeId": node_id,
            "text": str(metadata.get("content") or ""),
        }
    return {
        "kind": "text",
        "nodeId": node_id,
        "text": "",
        "metadata": metadata,
    }

