# rspaced

Holder for various implementations of our boot services.

## Contents

- **`rspaced_agent/`** — a bootc-based boot agent that replaces RHCOS in the OpenShift agent-based installer flow. Writes `rspacefs` as its repo and packages a chosen kernel derived from RHCOS.
- **`compose_rspaced/`** — build tool that consumes an existing OCP online repo and produces either an rspaced-based ISO (via bootc) or a signed bootc artifact compatible with the agent-based installer input surface.

See [`CLAUDE.md`](./CLAUDE.md) for the work plan and project conventions, and each subproject's `README.md` for component-specific details.
