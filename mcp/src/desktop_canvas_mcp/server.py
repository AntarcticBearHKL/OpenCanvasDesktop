"""MCP tool surface for the OpenCanvas desktop app.

This mirrors the web MCP's canvas tools, but the transport is stdio and the
bridge is the desktop app's loopback HTTP endpoint instead of a browser page.
Every mutating tool writes the whole project back through the bridge, so the
live Slint document, the autosave file and the library index stay in step.
"""

from __future__ import annotations

import base64
import json
from pathlib import Path
from typing import Any

from mcp import types
from mcp.server.mcpserver import MCPServer

from . import actions, bridge, canvas

_INSTRUCTIONS = Path(__file__).with_name("instructions.md").read_text(encoding="utf-8")
SERVER_VERSION = "0.1.0"
DEFAULT_MAX_CONTENT_BYTES = 16 * 1024 * 1024

server = MCPServer(
    "desktop-canvas",
    version=SERVER_VERSION,
    instructions=_INSTRUCTIONS,
)


# --------------------------------------------------------------- helpers
def _load(project_id: str) -> dict[str, Any]:
    return bridge.get_project(project_id)


def _commit(project: dict[str, Any], result: dict[str, Any]) -> dict[str, Any]:
    bridge.put_project(project)
    if result.get("selectionChanged"):
        state = bridge.state()
        if state.get("openProjectId") == project.get("id"):
            bridge.set_selection(project["id"], list(result.get("selected") or []))
    return project


def _apply(project_id: str, ops: list[dict[str, Any]]) -> tuple[dict[str, Any], dict[str, Any]]:
    project = _load(project_id)
    result = canvas.apply_ops(project, ops)
    _commit(project, result)
    return project, result


def _node(project: dict[str, Any], node_id: str) -> dict[str, Any]:
    return canvas.find_node(project, node_id)


def _ensure_group(library_id: str | None) -> str:
    if library_id:
        return library_id
    groups = bridge.library().get("groups") or []
    if groups:
        return str(groups[0]["id"])
    created = bridge.create_group("Project 1")
    return str(created["id"])


def _group_names() -> dict[str, str]:
    return {
        str(group.get("id")): str(group.get("name") or "")
        for group in bridge.library().get("groups") or []
    }


def _page_size(page: float | None, page_size: float | None) -> tuple[int, int]:
    page_number = max(1, int(page or 1))
    size = max(1, min(500, int(page_size or 50)))
    return page_number, size


def _content_meta(data: dict[str, Any]) -> types.TextContent:
    meta = {key: value for key, value in data.items() if key not in {"base64", "text"}}
    return types.TextContent(type="text", text=json.dumps(meta, ensure_ascii=False))


def _clean(value: dict[str, Any]) -> dict[str, Any]:
    cleaned: dict[str, Any] = {}
    for key, item in value.items():
        if item is None or item == "":
            continue
        if key == "count" and isinstance(item, (int, float)):
            cleaned[key] = int(round(float(item)))
        else:
            cleaned[key] = item
    return cleaned


def _generation_flow(
    project_id: str,
    *,
    prompt: str,
    mode: str,
    title: str | None,
    x: float | None,
    y: float | None,
    reference_node_ids: list[str] | None,
    options: dict[str, Any],
    auto_run: bool,
    parent_attempt_id: str | None = None,
) -> dict[str, Any]:
    if mode != "image":
        raise ValueError("Text and video workflows were removed; only Image Attempt is supported")
    project = _load(project_id)
    parent = None
    parent_meta: dict[str, Any] = {}
    if parent_attempt_id:
        parent = canvas.find_node(project, parent_attempt_id)
        if parent.get("type") != "image-attempt":
            raise ValueError(f"{parent_attempt_id} is not an Image Attempt")
        parent_meta = parent.get("metadata") or {}
        base_x = canvas._number(parent.get("position", {}).get("x"))
        base_y = canvas._number(parent.get("position", {}).get("y")) + canvas._number(parent.get("height")) + 40.0
        has_position = True
    else:
        base_x = canvas.next_canvas_x(project.get("nodes") or []) if x is None else float(x)
        base_y = 0.0 if y is None else float(y)
        has_position = x is not None or y is not None
    attempt_id = canvas.new_id("image-attempt")
    versions = [
        int((node.get("metadata") or {}).get("attemptVersion") or 0)
        for node in project.get("nodes") or []
        if node.get("type") == "image-attempt"
    ]
    version = max(versions, default=0) + 1
    parent_version = int((parent_meta or {}).get("attemptVersion") or 0)
    default_reason = f"Iteration from v{parent_version}" if parent_version else "Initial version"
    inherited = {
        key: parent_meta.get(key)
        for key in ("model", "size", "quality", "count", "background", "generationMode")
        if parent_meta.get(key) is not None
    }
    if parent_attempt_id and not title:
        title = f"v{version} · iteration"
    ops: list[dict[str, Any]] = [
        {
            "type": "add_node",
            "id": attempt_id,
            "nodeType": "image-attempt",
            "title": title or "Image Attempt",
            "position": {"x": base_x, "y": base_y} if has_position else None,
            "metadata": _clean(
                {
                    **inherited,
                    "prompt": prompt,
                    "status": "idle",
                    "generationMode": "image",
                    "attemptVersion": version,
                    "attemptReason": reason or default_reason,
                    "attemptFrozen": False,
                    "attemptRuns": [],
                    **options,
                }
            ),
        },
    ]
    for reference_id in reference_node_ids or []:
        ops.append({"type": "connect_nodes", "fromNodeId": reference_id, "toNodeId": attempt_id})
    ops.append({"type": "select_nodes", "ids": [attempt_id]})
    result = canvas.apply_ops(project, ops)
    if result.get("warnings"):
        result["warnings"] = [warning for warning in result["warnings"] if "节点不存在" not in warning]
    _commit(project, result)
    started: dict[str, Any] = {"started": False}
    if auto_run:
        try:
            started = bridge.generate(project_id, attempt_id)
        except bridge.BridgeError as error:
            started = {"started": False, "error": str(error)}
    return {
        "ok": True,
        "projectId": project_id,
        "promptNodeId": None,
        "generationNodeId": attempt_id,
        "attemptNodeId": attempt_id,
        "parentAttemptId": parent_attempt_id,
        "attemptVersion": version,
        "createdNodeIds": [attempt_id],
        "started": bool(started.get("started")),
        "generation": started,
        "warnings": result.get("warnings") or [],
    }


def _tool_delete(project_id: str, ids: list[str]) -> dict[str, Any]:
    project, result = _apply(project_id, [{"type": "delete_node", "ids": ids}])
    return {
        "ok": True,
        "projectId": project_id,
        "deleted": ids,
        "warnings": result.get("warnings") or [],
    }


# --------------------------------------------------------------- discovery
@server.tool()
def app_get_state() -> dict:
    """读取桌面端当前状态：屏幕（library/editor/image）、打开的画布、选中节点、节点/连线数量与可用操作命名空间。操作画布前先调用本工具。"""
    state = bridge.state()
    page = "canvas" if state.get("view") == "editor" else state.get("view")
    title = state.get("openProjectTitle") or ("画布库" if page == "library" else "图像工作室")
    return {
        "ok": True,
        "page": page,
        "title": title,
        "state": state,
        "availableActions": actions.AVAILABLE_ACTIONS,
    }


@server.tool()
def app_describe_actions(ns: str | None = None) -> dict:
    """读取 canvas 操作命名空间的 schema/说明。省略 ns 返回全部；app_apply_ops 的 ops 字段以此为准。"""
    return actions.describe(ns)


@server.tool()
def app_apply_ops(projectId: str | None = None, ops: list[dict[str, Any]] | None = None) -> dict:
    """向指定画布提交一批扁平操作（每项含 type，可带 ns="canvas"）。projectId 省略时使用当前打开的画布。"""
    if not ops:
        raise ValueError("ops 不能为空")
    target = projectId
    if not target:
        state = bridge.state()
        target = state.get("openProjectId")
        if not target:
            raise ValueError("没有打开的画布；请提供 projectId")
    project, result = _apply(target, ops)
    generation_results = []
    for item in result.get("generation") or []:
        node_id = item.get("nodeId")
        if not node_id:
            continue
        try:
            generation_results.append(bridge.generate(target, node_id))
        except bridge.BridgeError as error:
            generation_results.append({"nodeId": node_id, "started": False, "error": str(error)})
    return {
        "ok": True,
        "projectId": target,
        "applied": len(ops),
        "createdNodeIds": result.get("createdIds") or [],
        "selectedNodeIds": result.get("selected") or [],
        "generation": generation_results,
        "warnings": result.get("warnings") or [],
    }


@server.tool()
def app_screenshot(studio: str | None = None) -> list[types.ContentBlock]:
    """截取桌面应用当前窗口并返回 PNG（整个窗口，不是浏览器页面）。studio 参数仅为兼容 web 版保留，桌面端不支持切换工作区截图。"""
    del studio
    png = bridge.screenshot()
    return [
        types.ImageContent(
            type="image",
            data=base64.b64encode(png).decode("ascii"),
            mimeType="image/png",
        ),
        types.TextContent(type="text", text=f"OpenCanvas desktop window screenshot ({len(png)} bytes)"),
    ]


# --------------------------------------------------------------- library
@server.tool()
def canvas_list_projects(
    keyword: str | None = None,
    page: float | None = None,
    pageSize: float | None = None,
) -> dict:
    """列出桌面端全部画布（canvas 项目），含所属库（Project 分组）、节点数、连线数；支持 keyword 与分页。返回的 id 用作其他画布工具的 projectId。"""
    library = bridge.library()
    groups = {str(group.get("id")): str(group.get("name") or "") for group in library.get("groups") or []}
    rows = [
        row
        for row in library.get("projects") or []
        if row.get("kind", "canvas") == "canvas"
    ]
    if keyword:
        needle = keyword.strip().lower()
        rows = [
            row
            for row in rows
            if needle in str(row.get("title") or "").lower()
            or needle in groups.get(str(row.get("groupId")), "").lower()
        ]
    page_number, size = _page_size(page, pageSize)
    start = (page_number - 1) * size
    selected = rows[start : start + size]
    items = []
    for row in selected:
        node_count = None
        connection_count = None
        try:
            project = bridge.get_project(str(row["id"]))
            node_count = len(project.get("nodes") or [])
            connection_count = len(project.get("connections") or [])
        except bridge.BridgeError:
            pass
        items.append(
            {
                "id": row.get("id"),
                "title": row.get("title"),
                "groupId": row.get("groupId"),
                "libraryId": row.get("groupId"),
                "groupName": groups.get(str(row.get("groupId"))),
                "createdAt": row.get("createdAt"),
                "updatedAt": row.get("updatedAt"),
                "nodeCount": node_count,
                "connectionCount": connection_count,
                "open": library.get("openProjectId") == row.get("id"),
            }
        )
    return {"total": len(rows), "page": page_number, "pageSize": size, "items": items}


@server.tool()
def canvas_get_project(projectId: str) -> dict:
    """读取一个画布的完整 JSON（节点、连线、视口、库归属），用于检查或自行构造 app_apply_ops。"""
    project = _load(projectId)
    return {
        "ok": True,
        "projectId": projectId,
        "project": project,
        "nodeCount": len(project.get("nodes") or []),
        "connectionCount": len(project.get("connections") or []),
    }


@server.tool()
def canvas_create_project(title: str | None = None, libraryId: str | None = None) -> dict:
    """新建画布；libraryId 指定所属库（Project 分组），省略则放在第一个库（没有库时自动建一个）。返回新画布 id。"""
    group_id = _ensure_group(libraryId)
    project = bridge.create_project(title, group_id)
    groups = _group_names()
    return {
        "ok": True,
        "id": project["id"],
        "title": project.get("title"),
        "libraryId": group_id,
        "libraryName": groups.get(group_id),
        "projectId": project["id"],
    }


@server.tool()
def canvas_rename_project(id: str, title: str) -> dict:
    """重命名指定画布。"""
    project = bridge.patch_project(id, {"title": title})
    return {"ok": True, "id": id, "title": project.get("title")}


@server.tool()
def canvas_delete_projects(ids: list[str]) -> dict:
    """删除一个或多个画布。桌面端直接删除项目文件（没有回收站）；删除已打开的画布会先回到库。"""
    deleted = []
    for project_id in ids:
        bridge.delete_project(project_id)
        deleted.append(project_id)
    return {"ok": True, "deleted": deleted}


@server.tool()
def canvas_move_project_to_library(id: str, libraryId: str | None = None) -> dict:
    """把画布移动到指定库；libraryId 传 null/省略表示移出库分组。"""
    group_id = _ensure_group(libraryId) if libraryId else None
    project = bridge.patch_project(id, {"groupId": group_id})
    return {"ok": True, "id": id, "libraryId": group_id, "project": project}


@server.tool()
def canvas_create_library(name: str | None = None) -> dict:
    """新建库（桌面端的 Project 分组）。返回新库 id。"""
    group = bridge.create_group(name)
    return {"ok": True, "id": group["id"], "name": group.get("name"), "libraryId": group["id"]}


@server.tool()
def canvas_rename_library(id: str, name: str) -> dict:
    """重命名指定库。"""
    group = bridge.rename_group(id, name)
    return {"ok": True, "id": id, "name": group.get("name")}


@server.tool()
def canvas_delete_library(id: str) -> dict:
    """删除指定库；库内画布和图像项目会一并删除（桌面端没有回收站）。"""
    result = bridge.delete_group(id)
    return {"ok": True, "id": id, **result}


# --------------------------------------------------------------- nodes
@server.tool()
def canvas_create_node(
    projectId: str,
    nodeType: str,
    title: str | None = None,
    x: float | None = None,
    y: float | None = None,
    width: float | None = None,
    height: float | None = None,
    metadata: dict[str, Any] | None = None,
) -> dict:
    """创建任意类型节点。nodeType 可为 text、prompt、video-prompt、image、video、image-generation、video-generation、assets、image-stack、smart-canvas。x/y 为左上角坐标，省略时放在现有节点右侧。"""
    op: dict[str, Any] = {
        "type": "add_node",
        "nodeType": nodeType,
        "title": title,
        "x": x,
        "y": y,
        "width": width,
        "height": height,
        "metadata": metadata,
    }
    project, result = _apply(projectId, [op])
    node_id = (result.get("createdIds") or [None])[0]
    if not node_id:
        raise ValueError("节点创建失败")
    return {
        "ok": True,
        "projectId": projectId,
        "id": node_id,
        "node": _node(project, node_id),
        "warnings": result.get("warnings") or [],
    }


@server.tool()
def canvas_create_text_node(
    projectId: str,
    text: str | None = None,
    title: str | None = None,
    x: float | None = None,
    y: float | None = None,
    width: float | None = None,
    height: float | None = None,
) -> dict:
    """在当前画布创建单个文本节点。"""
    op: dict[str, Any] = {
        "type": "add_node",
        "nodeType": "text",
        "title": title,
        "x": x,
        "y": y,
        "width": width,
        "height": height,
        "metadata": {"content": text or "", "status": "success", "fontSize": 14},
    }
    project, result = _apply(projectId, [op])
    node_id = (result.get("createdIds") or [None])[0]
    if not node_id:
        raise ValueError("文本节点创建失败")
    return {"ok": True, "projectId": projectId, "id": node_id, "node": _node(project, node_id)}


@server.tool()
def canvas_create_text_nodes(
    projectId: str,
    items: list[dict[str, Any]],
    x: float | None = None,
    y: float | None = None,
    gap: float | None = None,
    direction: str | None = None,
) -> dict:
    """批量创建文本节点；items 每项为 {text,title?,x?,y?,width?,height?}。direction 为 row/column，省略按 column 排列。"""
    if not items:
        raise ValueError("items 不能为空")
    gap_value = float(gap or 40.0)
    row = direction == "row"
    ops: list[dict[str, Any]] = []
    for index, item in enumerate(items):
        item_x = item.get("x")
        item_y = item.get("y")
        if item_x is None and item_y is None and x is None and y is None:
            position: dict[str, float] | None = None
        else:
            position = {
                "x": float(item_x if item_x is not None else (x or 0.0) + (index * (340.0 + gap_value) if row else 0.0)),
                "y": float(item_y if item_y is not None else (y or 0.0) + (0.0 if row else index * (360.0 + gap_value))),
            }
        ops.append(
            {
                "type": "add_node",
                "nodeType": "text",
                "title": item.get("title"),
                "position": position,
                "width": item.get("width"),
                "height": item.get("height"),
                "metadata": {"content": item.get("text") or "", "status": "success", "fontSize": 14},
            }
        )
    project, result = _apply(projectId, ops)
    return {
        "ok": True,
        "projectId": projectId,
        "ids": result.get("createdIds") or [],
        "nodes": [_node(project, node_id) for node_id in result.get("createdIds") or []],
    }


@server.tool()
def canvas_update_node(
    projectId: str,
    id: str,
    patch: dict[str, Any] | None = None,
    metadata: dict[str, Any] | None = None,
) -> dict:
    """更新节点基础字段（title/position/width/height）或 metadata；metadata 值为 null 时删除该键。"""
    project, _ = _apply(
        projectId,
        [{"type": "update_node", "id": id, "patch": patch or {}, "metadata": metadata or {}}],
    )
    return {"ok": True, "projectId": projectId, "node": _node(project, id)}


@server.tool()
def canvas_update_node_text(projectId: str, id: str, text: str, title: str | None = None) -> dict:
    """更新文本节点内容和可选标题。"""
    patch = {"title": title} if title else {}
    project, _ = _apply(
        projectId,
        [
            {
                "type": "update_node",
                "id": id,
                "patch": patch,
                "metadata": {"content": text, "status": "success"},
            }
        ],
    )
    return {"ok": True, "projectId": projectId, "node": _node(project, id)}


@server.tool()
def canvas_move_nodes(projectId: str, items: list[dict[str, Any]]) -> dict:
    """移动节点；每项为 {id,x?,y?,dx?,dy?}，有 x/y 时使用绝对坐标（左上角），否则按 dx/dy 偏移。"""
    ops = []
    for item in items:
        if item.get("x") is not None or item.get("y") is not None:
            node = _node(_load(projectId), str(item.get("id")))
            current = node.get("position") or {}
            ops.append(
                {
                    "type": "update_node",
                    "id": item["id"],
                    "patch": {
                        "position": {
                            "x": float(item["x"]) if item.get("x") is not None else float(current.get("x") or 0.0),
                            "y": float(item["y"]) if item.get("y") is not None else float(current.get("y") or 0.0),
                        }
                    },
                }
            )
        else:
            ops.append(
                {
                    "type": "move_node",
                    "id": item["id"],
                    "dx": float(item.get("dx") or 0.0),
                    "dy": float(item.get("dy") or 0.0),
                }
            )
    project, _ = _apply(projectId, ops)
    return {"ok": True, "projectId": projectId, "ids": [item["id"] for item in items]}


@server.tool()
def canvas_resize_node(
    projectId: str,
    id: str,
    width: float,
    height: float,
    freeResize: bool | None = None,
) -> dict:
    """调整节点尺寸；图片节点会锁定图片比例，freeResize 仅保留兼容。"""
    metadata = {"freeResize": freeResize} if freeResize is not None else {}
    project, _ = _apply(
        projectId,
        [
            {
                "type": "update_node",
                "id": id,
                "patch": {"width": width, "height": height},
                "metadata": metadata,
            }
        ],
    )
    return {"ok": True, "projectId": projectId, "node": _node(project, id)}


@server.tool()
def canvas_set_node_flags(
    projectId: str,
    ids: list[str],
    locked: bool | None = None,
    hidden: bool | None = None,
) -> dict:
    """设置节点锁定/隐藏状态；locked 与 hidden 至少填写一个。"""
    metadata: dict[str, Any] = {}
    if locked is not None:
        metadata["locked"] = locked
    if hidden is not None:
        metadata["hidden"] = hidden
    if not metadata:
        raise ValueError("locked 与 hidden 至少需要一个")
    project, _ = _apply(
        projectId,
        [{"type": "update_node", "id": node_id, "metadata": metadata} for node_id in ids],
    )
    return {"ok": True, "projectId": projectId, "ids": ids}


@server.tool()
def canvas_bulk_rename(projectId: str, ids: list[str], title: str) -> dict:
    """批量重命名节点；多个节点按给定顺序编号为「标题 1」「标题 2」…，单个节点直接使用标题。"""
    clean_title = title.strip()
    if not clean_title:
        raise ValueError("title 不能为空")
    ops = [
        {
            "type": "update_node",
            "id": node_id,
            "patch": {"title": f"{clean_title} {index + 1}" if len(ids) > 1 else clean_title},
        }
        for index, node_id in enumerate(ids)
    ]
    project, _ = _apply(projectId, ops)
    return {"ok": True, "projectId": projectId, "ids": ids}


@server.tool()
def canvas_align_nodes(projectId: str, ids: list[str], mode: str) -> dict:
    """按选区包围盒对齐或分布节点：left/center-x/right/top/center-y/bottom 对齐，distribute-x/distribute-y 等距分布（分布需 3 个以上节点）。"""
    project, _ = _apply(projectId, [{"type": "align_nodes", "ids": ids, "axis": mode}])
    return {"ok": True, "projectId": projectId, "nodes": [_node(project, node_id) for node_id in ids]}


@server.tool()
def canvas_duplicate_node(
    projectId: str,
    id: str,
    dx: float | None = None,
    dy: float | None = None,
) -> dict:
    """复制节点；默认在原节点右下 36px 创建副本。"""
    project, result = _apply(
        projectId,
        [{"type": "duplicate_node", "id": id, "dx": dx, "dy": dy}],
    )
    new_id = (result.get("createdIds") or [None])[0]
    if not new_id:
        raise ValueError("复制节点失败")
    return {"ok": True, "projectId": projectId, "id": new_id, "node": _node(project, new_id)}


@server.tool()
def canvas_delete_nodes(projectId: str, ids: list[str]) -> dict:
    """删除指定节点及相关连线。"""
    return _tool_delete(projectId, ids)


@server.tool()
def canvas_connect_nodes(projectId: str, connections: list[dict[str, Any]]) -> dict:
    """批量连接节点；每项为 {fromNodeId,toNodeId,relation?}。"""
    project, result = _apply(
        projectId,
        [
            {
                "type": "connect_nodes",
                "fromNodeId": item.get("fromNodeId"),
                "toNodeId": item.get("toNodeId"),
                "relation": item.get("relation"),
            }
            for item in connections
        ],
    )
    pairs = {(item.get("fromNodeId"), item.get("toNodeId")) for item in connections}
    created = [
        conn
        for conn in project.get("connections") or []
        if (conn.get("fromNodeId"), conn.get("toNodeId")) in pairs
    ]
    return {
        "ok": True,
        "projectId": projectId,
        "connections": created,
        "warnings": result.get("warnings") or [],
    }


@server.tool()
def canvas_select_nodes(projectId: str, ids: list[str]) -> dict:
    """设置当前选中节点（实时同步；目标画布必须已在桌面端打开）。"""
    state = bridge.state()
    if state.get("openProjectId") != projectId:
        raise ValueError("该画布未在桌面端打开，无法设置选中；请先打开它")
    return {"ok": True, **bridge.set_selection(projectId, ids)}


@server.tool()
def canvas_set_viewport(projectId: str, viewport: dict[str, float]) -> dict:
    """调整画布视口 {x,y,k}（k 为缩放，0.05..5）。"""
    project, _ = _apply(projectId, [{"type": "set_viewport", "viewport": viewport}])
    return {"ok": True, "projectId": projectId, "viewport": project.get("viewport")}


@server.tool()
def canvas_get_node_content(
    projectId: str,
    nodeId: str,
    itemId: str | None = None,
    maxBytes: int | None = None,
) -> list[types.ContentBlock]:
    """读取节点真实内容：图片返回图片块、文本返回文本块、视频返回嵌入二进制资源。图片组可用 itemId 选择其中一张。"""
    project = _load(projectId)
    data = canvas.node_content(project, nodeId, itemId)
    limit = maxBytes or DEFAULT_MAX_CONTENT_BYTES
    if data.get("kind") == "text":
        metadata = {key: value for key, value in data.items() if key != "text"}
        return [
            types.TextContent(type="text", text=json.dumps(metadata, ensure_ascii=False)),
            types.TextContent(type="text", text=str(data.get("text") or "")),
        ]
    payload = str(data.get("base64") or "")
    decoded = len(payload) * 3 // 4
    if decoded > limit:
        raise ValueError(f"节点内容超过大小上限：{decoded} bytes > {limit}")
    mime = str(data.get("mimeType") or "application/octet-stream")
    if data.get("kind") == "image":
        return [
            _content_meta(data),
            types.ImageContent(type="image", data=payload, mimeType=mime),
        ]
    resource = types.BlobResourceContents(
        uri=f"canvas://{projectId}/{nodeId}",
        mimeType=mime,
        blob=payload,
    )
    return [
        _content_meta(data),
        types.EmbeddedResource(type="resource", resource=resource),
    ]


@server.tool()
def canvas_get_image_group(projectId: str, nodeId: str) -> dict:
    """读取图片组节点的图片清单；随后可用 canvas_get_node_content 配 itemId 读取某张图。"""
    project = _load(projectId)
    node = _node(project, nodeId)
    if node.get("type") != "image-stack":
        raise ValueError(f"节点 {nodeId} 不是图片组")
    metadata = node.get("metadata") or {}
    images = metadata.get("images") or []
    return {
        "ok": True,
        "nodeId": nodeId,
        "count": len(images),
        "primaryImageId": metadata.get("primaryImageId"),
        "images": [
            {
                "id": image.get("id"),
                "status": image.get("status"),
                "primary": image.get("id") == metadata.get("primaryImageId"),
                "naturalWidth": image.get("naturalWidth"),
                "naturalHeight": image.get("naturalHeight"),
                "bytes": image.get("bytes"),
                "mimeType": image.get("mimeType"),
                "hasContent": bool(image.get("content")),
            }
            for image in images
        ],
    }


# --------------------------------------------------------------- generation
@server.tool()
def canvas_create_generation_flow(
    projectId: str,
    prompt: str,
    mode: str | None = None,
    title: str | None = None,
    x: float | None = None,
    y: float | None = None,
    referenceNodeIds: list[str] | None = None,
    autoRun: bool | None = None,
    model: str | None = None,
    size: str | None = None,
    quality: str | None = None,
    count: float | None = None,
    seconds: str | None = None,
    vquality: str | None = None,
    generateAudio: str | None = None,
    watermark: str | None = None,
    videoMode: str | None = None,
) -> dict:
    """创建 Image Attempt：一个节点承载 prompt/config/运行结果；参考图片接入该节点，可选立即触发生成。文本与视频流程已移除。"""
    options = _clean(
        {
            "model": model,
            "size": size,
            "quality": quality,
            "count": count,
            "seconds": seconds,
            "vquality": vquality,
            "generateAudio": generateAudio,
            "watermark": watermark,
            "videoMode": videoMode,
        }
    )
    return _generation_flow(
        projectId,
        prompt=prompt,
        mode=mode or "image",
        title=title,
        x=x,
        y=y,
        reference_node_ids=referenceNodeIds,
        options=options,
        auto_run=bool(autoRun),
    )


@server.tool()
def canvas_create_image_prompt_flow(
    projectId: str,
    prompt: str,
    title: str | None = None,
    x: float | None = None,
    y: float | None = None,
    referenceNodeIds: list[str] | None = None,
    parentAttemptId: str | None = None,
    reason: str | None = None,
    autoRun: bool | None = None,
    model: str | None = None,
    size: str | None = None,
    quality: str | None = None,
    count: float | None = None,
) -> dict:
    """创建 Image Attempt 并接入参考图片，可选立即触发生图。"""
    return _generation_flow(
        projectId,
        prompt=prompt,
        mode="image",
        title=title,
        x=x,
        y=y,
        reference_node_ids=referenceNodeIds,
        parent_attempt_id=parentAttemptId,
        reason=reason,
        options=_clean({"model": model, "size": size, "quality": quality, "count": count}),
        auto_run=bool(autoRun),
    )


@server.tool()
def canvas_generate_image(
    projectId: str,
    prompt: str,
    title: str | None = None,
    x: float | None = None,
    y: float | None = None,
    referenceNodeIds: list[str] | None = None,
    parentAttemptId: str | None = None,
    reason: str | None = None,
    model: str | None = None,
    size: str | None = None,
    quality: str | None = None,
    count: float | None = None,
) -> dict:
    """创建 Image Attempt 并立即触发生成。目标画布必须已在桌面端打开。"""
    return _generation_flow(
        projectId,
        prompt=prompt,
        mode="image",
        title=title,
        x=x,
        y=y,
        reference_node_ids=referenceNodeIds,
        parent_attempt_id=parentAttemptId,
        reason=reason,
        options=_clean({"model": model, "size": size, "quality": quality, "count": count}),
        auto_run=True,
    )


@server.tool()
def canvas_iterate_attempt(
    projectId: str,
    nodeId: str,
    prompt: str,
    title: str | None = None,
    reason: str | None = None,
    referenceNodeIds: list[str] | None = None,
    model: str | None = None,
    size: str | None = None,
    quality: str | None = None,
    count: float | None = None,
    autoRun: bool | None = True,
) -> dict:
    """Create the next Image Attempt version below the given one, preserving the old version. No version-to-version wire is added."""
    return _generation_flow(
        projectId,
        prompt=prompt,
        mode="image",
        title=title,
        x=None,
        y=None,
        reference_node_ids=referenceNodeIds,
        parent_attempt_id=nodeId,
        reason=reason,
        options=_clean({"model": model, "size": size, "quality": quality, "count": count}),
        auto_run=bool(autoRun),
    )


@server.tool()
def canvas_run_generation(
    projectId: str,
    nodeId: str,
    mode: str | None = None,
    prompt: str | None = None,
    agentTrace: str | None = None,
    agentSteps: list[dict] | None = None,
    agentRunId: str | None = None,
    versionReason: str | None = None,
) -> dict:
    """触发指定 Image Attempt 生成。桌面端要求该画布已在窗口中打开；prompt 用于覆盖尝试的提示词。agentTrace/agentSteps 用于写入 agent 的原始思考与结构化步骤。"""
    del mode
    if prompt is not None or versionReason is not None:
        project = _load(projectId)
        node = _node(project, nodeId)
        if prompt is not None and node.get("type") in canvas.PROMPT_TYPES:
            node.setdefault("metadata", {})["prompt"] = prompt
        if versionReason is not None and node.get("type") == "image-attempt":
            node.setdefault("metadata", {})["attemptReason"] = versionReason
        bridge.put_project(project)
    payload = {
        key: value
        for key, value in {
            "agentTrace": agentTrace,
            "agentSteps": agentSteps,
            "agentRunId": agentRunId,
            "versionReason": versionReason,
        }.items()
        if value is not None
    }
    return {
        "ok": True,
        "projectId": projectId,
        **bridge.generate(projectId, nodeId, payload or None),
    }


@server.tool()
def generation_get_status(
    scope: str | None = None,
    taskId: str | None = None,
    nodeIds: list[str] | None = None,
    limit: float | None = None,
) -> dict:
    """查询桌面端生成运行时状态：当前运行节点、运行时是否就绪、打开的画布。桌面端一次只跑一个生成任务，没有 web 版的任务列表。"""
    del scope, taskId, nodeIds, limit
    state = bridge.generation_status()
    return {"ok": True, **state}


# --------------------------------------------------------------- image groups
@server.tool()
def canvas_group_images(
    projectId: str,
    nodeIds: list[str],
    title: str | None = None,
    x: float | None = None,
    y: float | None = None,
    removeSources: bool | None = None,
) -> dict:
    """把多个图片节点（或已有图片组）里的图片打包成一个 image-stack；默认移除来源节点。x/y 为新图片组中心。"""
    position = None
    if x is not None or y is not None:
        position = {"x": float(x or 0.0), "y": float(y or 0.0)}
    project, result = _apply(
        projectId,
        [
            {
                "type": "pack_images_into_group",
                "nodeIds": nodeIds,
                "title": title,
                "position": position,
                "removeSources": removeSources,
            }
        ],
    )
    node_id = (result.get("createdIds") or [None])[0]
    if not node_id:
        raise ValueError("图片组创建失败")
    return {"ok": True, "projectId": projectId, "id": node_id, "node": _node(project, node_id)}


@server.tool()
def canvas_ungroup_images(
    projectId: str,
    nodeId: str,
    gap: float | None = None,
    columns: float | None = None,
    keepGroup: bool | None = None,
) -> dict:
    """把图片组节点拆成多个独立图片节点；默认删除图片组。"""
    project, result = _apply(
        projectId,
        [
            {
                "type": "unpack_image_group",
                "nodeId": nodeId,
                "gap": gap,
                "columns": columns,
                "keepGroup": keepGroup,
            }
        ],
    )
    ids = result.get("createdIds") or []
    return {
        "ok": True,
        "projectId": projectId,
        "ids": ids,
        "nodes": [_node(project, node_id) for node_id in ids],
    }
