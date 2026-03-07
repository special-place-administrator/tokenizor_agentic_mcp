# SpacetimeDB Module Scaffold

This directory is the expected local module path for the Tokenizor SpacetimeDB control plane.

Current status:

- the MCP binary now validates that this path exists
- deployment checks expect a local SpacetimeDB daemon and CLI
- schema/module deployment is not wired into the Rust binary yet

Planned responsibility for this path:

- hold the Tokenizor SpacetimeDB schema/module source
- support local `init` and future `migrate` workflows
- anchor schema versioning alongside the MCP server
