# compose_rspaced

Build tool for rspaced artifacts.

Given an existing OCP online repo as input, `compose_rspaced` produces one of:

1. **rspaced-based ISO** — built using bootc.
2. **Signed bootc artifact** — behaves like the OpenShift agent-based installer (same input config surface) but with signed bootc images on the inside instead of RHCOS.

Both targets are part of the new bootc-based, signed boot pipeline.

Implementation details (input-repo ingestion, per-target build pipelines) to follow as code lands.
