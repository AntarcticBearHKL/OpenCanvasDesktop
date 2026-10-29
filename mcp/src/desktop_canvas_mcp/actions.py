"""Static action catalogue for ``app_get_state`` / ``app_describe_actions``.

The desktop MCP does not have a browser-side action registry; this module is the
one authoritative list of canvas ops the Python reducer understands. It stays
deliberately close to the web app's `app_describe_actions` shape so an agent
that already knows the web MCP can use the desktop one unchanged.
"""

from __future__ import annotations

from typing import Any

OP_DESCRIPTIONS: dict[str, str] = {
    "add_node": "创建节点；position 为左上角坐标，省略时放在现有节点右侧。",
    "move_node": "按 dx/dy 移动节点。",
    "update_node": "更新节点 title/position/width/height 与 metadata；metadata 值为 null 时删除该键。",
    "delete_node": "删除节点及相关连线；可用 ids、id 或 nodeType 指定。",
    "delete_connections": "删除连线；可用 ids、id 或 all。",
    "connect_nodes": "连接两个节点；重复连接会复用已有连线。",
    "set_viewport": "设置画布视口 {x,y,k}，k 限制在 0.05..5。",
    "select_nodes": "设置当前选中节点（实时同步到桌面端窗口）。",
    "deselect_all": "清空选中。",
    "duplicate_node": "复制节点（默认右移/下移 36）。",
    "move_node_layer": "调整节点层级：direction 为 up/down。",
    "toggle_node_flag": "切换 metadata 中的布尔标记（如 locked/hidden）。",
    "rename_nodes": "批量重命名；多个节点自动编号。",
    "set_font_size": "设置文本节点字号。",
    "set_free_resize": "设置自由缩放；关闭图片自由缩放时按原始比例校正高度。",
    "resize_node": "直接设置节点宽高与可选位置。",
    "set_node_content": "写入文本/图片/视频节点的内容；提示词节点写入 prompt。",
    "set_node_prompt": "写入提示词节点的 prompt。",
    "set_node_title": "设置节点标题。",
    "apply_node_metadata": "合并节点 metadata。",
    "set_batch_primary": "设置批量文本/图片组的主项。",
    "align_nodes": "对齐或分布节点（left/center-x/right/top/center-y/bottom/distribute-x/distribute-y）。",
    "pack_images_into_group": "把图片节点/图片组打包成 image-stack。",
    "unpack_image_group": "把 image-stack 拆成独立图片节点。",
    "bind_video_slot": "绑定视频首帧/尾帧/参考槽位。",
    "clear_video_slot": "清除视频槽位。",
    "switch_video_frame_slot": "在首帧/尾帧槽位之间切换。",
    "set_video_mode": "设置视频提示词模式 frames/reference。",
    "set_asset_source": "设置资源节点来源 folder/cache/studio。",
    "set_zoom": "只调整视口缩放 k。",
    "run_generation": "在已打开的画布上触发节点生成；桌面端要求该画布已打开。",
}

CANVAS_OPS = [
    {
        "type": name,
        "description": description,
        "properties": {
            "ns": {"type": "string", "enum": ["canvas"]},
            "type": {"type": "string", "const": name},
        },
    }
    for name, description in OP_DESCRIPTIONS.items()
]

AVAILABLE_ACTIONS: list[dict[str, Any]] = [
    {
        "ns": "canvas",
        "title": "画布",
        "description": "OpenCanvas 桌面端的节点、连线、选中、视口、图片组与生成操作。",
        "ops": [{"type": name, "description": description} for name, description in OP_DESCRIPTIONS.items()],
    }
]


def describe(namespace: str | None = None) -> dict[str, Any]:
    actions = [
        action
        for action in AVAILABLE_ACTIONS
        if namespace is None or action["ns"] == namespace
    ]
    return {"ok": True, "namespaces": actions}
