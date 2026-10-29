"""Entry point for the OpenCanvas desktop MCP server (stdio transport)."""

from __future__ import annotations

from .server import server


def main() -> None:
    server.run()


if __name__ == "__main__":
    main()
