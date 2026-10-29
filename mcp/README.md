# OpenCanvas Desktop MCP

Mirrors the web `canvas_*` MCP tool surface for the **native Rust + Slint**
OpenCanvas desktop app (`D:\Dev\open-canvas-desktop`).

Unlike the web MCP, there is no browser page and no SSE bridge. The desktop app
hosts a loopback HTTP bridge (default `127.0.0.1:3211`) that executes every
request on its UI thread, so the live document, autosave file and library index
stay in step. The MCP server runs over **stdio** and is registered directly in
opencode.

```
opencode ── stdio ──> desktop-canvas-mcp ── HTTP 127.0.0.1:3211 ──> OpenCanvas desktop app
                                                                    (UI thread / same App methods)
```

## Requirements

- The desktop app must be **running** (the bridge lives inside it).
- Python 3.12+ (`uv` makes this automatic).

## Setup

```powershell
cd D:\Dev\open-canvas-desktop\mcp
uv sync
uv run desktop-canvas-mcp
```

`uv run` is enough for normal use; `uv.lock` pins the environment.

Register with opencode:

```jsonc
"desktop-canvas": {
  "type": "local",
  "command": ["uv", "run", "--directory", "D:\\Dev\\open-canvas-desktop\\mcp", "desktop-canvas-mcp"],
  "enabled": true
}
```

## Bridge configuration (desktop side)

| Environment variable | Default | Meaning |
|---|---|---|
| `OC_AGENT_BRIDGE` | on | `0`/`false`/`off` disables the bridge |
| `OC_AGENT_BRIDGE_HOST` | `127.0.0.1` | Bind address |
| `OC_AGENT_BRIDGE_PORT` | `3211` | Port |
| `OC_AGENT_BRIDGE_TOKEN` | empty | Optional bearer token; set the same value in the MCP process |

The MCP process reads `OC_AGENT_BRIDGE_URL` (default `http://127.0.0.1:3211`),
`OC_AGENT_BRIDGE_TOKEN` and `OC_AGENT_BRIDGE_TIMEOUT`.

## Tool groups

- **Discovery** — `app_get_state`, `app_describe_actions`, `app_apply_ops`,
  `app_screenshot`
- **Library** — `canvas_list_projects`, `canvas_create_project`,
  `canvas_rename_project`, `canvas_delete_projects`,
  `canvas_move_project_to_library`, `canvas_create_library`,
  `canvas_rename_library`, `canvas_delete_library`
- **Nodes** — `canvas_create_node`, `canvas_create_text_node`,
  `canvas_create_text_nodes`, `canvas_update_node`, `canvas_update_node_text`,
  `canvas_move_nodes`, `canvas_resize_node`, `canvas_set_node_flags`,
  `canvas_bulk_rename`, `canvas_align_nodes`, `canvas_duplicate_node`,
  `canvas_delete_nodes`, `canvas_connect_nodes`, `canvas_select_nodes`,
  `canvas_set_viewport`
- **Content** — `canvas_get_project`, `canvas_get_node_content`,
  `canvas_get_image_group`
- **Image groups** — `canvas_group_images`, `canvas_ungroup_images`
- **Generation** — `canvas_create_generation_flow`,
  `canvas_create_image_prompt_flow`, `canvas_generate_image`,
  `canvas_run_generation`, `generation_get_status`

There is no pixel studio in the desktop port, so the web `canvas_*_pixel_*`
tools are intentionally absent. Generation requires the target canvas to be
open in the desktop window; other edits work on any project without switching
the UI.

## Differences from the web MCP

- `libraryId` is the desktop **Project** group; `projectId` is one canvas inside it.
- Deletion is permanent (no web recycle bin).
- `app_screenshot` captures the whole native window (PNG).
- A single generation runs at a time; `generation_get_status` reports the
  running node rather than a task list.
