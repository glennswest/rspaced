# rspaced_agent bootc — milestone 1: hello world

A minimal **bootc** app that boots and prints a banner to **console + serial**.
First step of `rspaced_agent` (see [`../DESIGN.md`](../DESIGN.md) build order).

- `Containerfile` — `FROM` a bootc base (default: the **rhel-coreos** image from
  our packed store, so the kernel is our content). Enables the hello unit.
- `kargs.d/10-console.toml` — `console=tty0 console=ttyS0,115200n8` the
  **bootc-native** way. **Never** coreos-installer.
- `hello-rspaced.service` — prints a boot banner to the console.
- `build.sh` — on rspaced.g8.lo: pull the base (authed), `podman build`, then
  `bootc-image-builder` → qcow2 (or `TYPE=iso`). Output under `/data/agent-out`.

```sh
./build.sh            # qcow2
TYPE=iso ./build.sh   # bootable ISO
```

Boot the artifact on snotest.g8.lo; expect the HELLO WORLD banner on the serial
console. Next milestones: kernel-from-rspacefs wiring, state check
(boot-media vs resident), disk find/format, rspacefs setup, pivot.
