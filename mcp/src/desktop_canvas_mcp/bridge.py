"""HTTP client for the OpenCanvas desktop agent bridge.

The desktop app hosts the bridge on loopback (default ``127.0.0.1:3211``).
The MCP server stays intentionally thin here: all canvas semantics live in
:mod:`desktop_canvas_mcp.canvas`, and the app only persists/syncs whole project
documents. That keeps the live Slint document and the on-disk JSON in step.
"""

from __future__ import annotations

import json
import os
import urllib.error
import urllib.request
from typing import Any

DEFAULT_URL = "http://127.0.0.1:3211"
DEFAULT_TIMEOUT = 30.0
SCREENSHOT_TIMEOUT = 60.0


class BridgeError(RuntimeError):
    """The desktop app is unreachable or answered with an error."""


def base_url() -> str:
    return os.environ.get("OC_AGENT_BRIDGE_URL", DEFAULT_URL).rstrip("/")


def _timeout() -> float:
    try:
        return float(os.environ.get("OC_AGENT_BRIDGE_TIMEOUT", DEFAULT_TIMEOUT))
    except ValueError:
        return DEFAULT_TIMEOUT


def request(
    method: str,
    path: str,
    payload: Any | None = None,
    *,
    raw: bool = False,
    timeout: float | None = None,
) -> Any:
    """Send one JSON (or raw) request to the bridge and decode the answer."""
    url = f"{base_url()}{path}"
    data = None
    headers = {"Accept": "application/json, image/png"}
    if payload is not None:
        data = json.dumps(payload, ensure_ascii=False).encode("utf-8")
        headers["Content-Type"] = "application/json; charset=utf-8"
    token = os.environ.get("OC_AGENT_BRIDGE_TOKEN", "").strip()
    if token:
        headers["Authorization"] = f"Bearer {token}"
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=timeout or _timeout()) as response:
            body = response.read()
            if raw:
                return body
            if not body:
                return {}
            return json.loads(body.decode("utf-8"))
    except urllib.error.HTTPError as exc:
        body = exc.read().decode("utf-8", "replace")
        try:
            detail = json.loads(body).get("error") or body
        except (ValueError, AttributeError):
            detail = body
        raise BridgeError(f"desktop bridge HTTP {exc.code}: {detail}") from exc
    except urllib.error.URLError as exc:
        raise BridgeError(
            "OpenCanvas 桌面端没有响应。请先启动 open-canvas-desktop（代理桥默认 "
            f"{base_url()}）；如果改过端口，设置 OC_AGENT_BRIDGE_URL。原因：{exc.reason}"
        ) from exc
    except TimeoutError as exc:
        raise BridgeError(f"OpenCanvas 桌面端在 {timeout or _timeout():.0f}s 内没有响应") from exc


def health() -> dict:
    return request("GET", "/health")


def state() -> dict:
    return request("GET", "/state")


def library() -> dict:
    return request("GET", "/library")


def get_project(project_id: str) -> dict:
    return request("GET", f"/project/{project_id}")


def put_project(project: dict) -> dict:
    return request("PUT", f"/project/{project['id']}", project)


def create_project(title: str | None, group_id: str | None) -> dict:
    payload: dict[str, Any] = {}
    if title:
        payload["title"] = title
    if group_id:
        payload["groupId"] = group_id
    return request("POST", "/library/projects", payload)


def patch_project(project_id: str, changes: dict) -> dict:
    return request("PATCH", f"/library/projects/{project_id}", changes)


def delete_project(project_id: str) -> dict:
    return request("DELETE", f"/library/projects/{project_id}")


def create_group(name: str | None) -> dict:
    return request("POST", "/library/groups", {"name": name} if name else {})


def rename_group(group_id: str, name: str) -> dict:
    return request("PATCH", f"/library/groups/{group_id}", {"name": name})


def delete_group(group_id: str) -> dict:
    return request("DELETE", f"/library/groups/{group_id}")


def open_project(project_id: str) -> dict:
    return request("POST", f"/project/{project_id}/open", {})


def set_selection(project_id: str, ids: list[str]) -> dict:
    # `project_id` is intentionally unused by the bridge: selection belongs to
    # the open document, and the call is rejected when it is a different one.
    del project_id
    return request("POST", "/selection", {"ids": ids})


def generate(
    project_id: str,
    node_id: str,
    payload: dict | None = None,
) -> dict:
    body = {"nodeId": node_id}
    if payload:
        body.update(payload)
    return request("POST", f"/project/{project_id}/generate", body)


def cancel(project_id: str, node_id: str) -> dict:
    return request("POST", f"/project/{project_id}/cancel", {"nodeId": node_id})


def generation_status() -> dict:
    return request("GET", "/generation")


def screenshot() -> bytes:
    return request("GET", "/screenshot", raw=True, timeout=SCREENSHOT_TIMEOUT)
