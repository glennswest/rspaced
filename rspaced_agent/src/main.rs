//! rspaced_agent — the boot agent that runs inside the RHCOS initramfs.
//!
//! It boots on the *same* RHCOS kernel OpenShift uses (no stub, no kexec): the
//! kernel + this initramfs come straight from the pulled payload. From here the
//! agent will check state (boot-media vs resident), find/format the disk, set
//! up rspacefs, mount the content-addressed root the composefs way (verity
//! digest pinned on the cmdline) and `switch_root` in — all on this one kernel.
//!
//! Milestone 1 (this file): prove we booted by writing a banner to the console
//! (serial too, via `console=ttyS0` on the kernel command line).

use std::fs::{self, OpenOptions};
use std::io::Write;

fn main() {
    let kver = fs::read_to_string("/proc/version").unwrap_or_else(|_| "unknown\n".into());
    let cmdline = fs::read_to_string("/proc/cmdline").unwrap_or_else(|_| "unknown\n".into());

    let banner = format!(
        "\n\
==========================================================\n\
  HELLO WORLD — rspaced_agent is alive in the initramfs\n\
  kernel : {}\n\
  cmdline: {}\n\
==========================================================\n",
        kver.trim(),
        cmdline.trim(),
    );

    // stdout (captured by the journal / console).
    print!("{banner}");
    let _ = std::io::stdout().flush();

    // Write straight to /dev/console too, so it lands on the serial console
    // regardless of how stdio is wired this early in boot.
    if let Ok(mut console) = OpenOptions::new().write(true).open("/dev/console") {
        let _ = console.write_all(banner.as_bytes());
        let _ = console.flush();
    }
}
