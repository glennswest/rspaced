# rspaced_agent

A bootc-based implementation of the boot agent.

- Writes `rspacefs` as its repo.
- Packages a chosen version of kernel (and supporting userspace) derived from RHCOS.
- Acts as a drop-in replacement for the RHCOS format / RHCOS boot used by the OpenShift agent-based installer (ABI).

Implementation details (Containerfile, `rspacefs` writer, kernel packaging path) to follow as code lands.
