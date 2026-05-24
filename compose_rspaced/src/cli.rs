use anyhow::Result;
use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "compose_rspaced",
    version,
    about = "Compose rspaced boot artifacts from RHCOS install media."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Print the latest RHCOS z-stream version in a major.minor series.
    Latest(LatestArgs),
    /// Push the artifact set to a qregistry (the offline cache / source of truth).
    Registry(SourceArgs),
    /// Build a bootc-based live ISO.
    Iso(OutArgs),
    /// Build a PXE-bootable tree (kernel + initrd + rootfs reference).
    Pxe(OutArgs),
    /// Emit the metal raw disk image.
    Raw(OutArgs),
    /// Emit the QEMU qcow2 image.
    Qcow2(OutArgs),
    /// Write the raw RHCOS files to a directory (debugging / inspection).
    Files(OutArgs),
}

#[derive(Args)]
pub struct LatestArgs {
    /// major.minor series, e.g. "4.18".
    #[arg(long, default_value = "4.18")]
    pub series: String,
    /// Target architecture (kernel-arch name, e.g. x86_64 / aarch64).
    #[arg(long, default_value = "x86_64")]
    pub arch: String,
}

#[derive(Args, Clone)]
pub struct SourceArgs {
    /// Exact RHCOS z-stream version (e.g. 4.18.27). Overrides --series.
    #[arg(long)]
    pub version: Option<String>,
    /// major.minor series to resolve "latest" from when --version is absent.
    #[arg(long, default_value = "4.18")]
    pub series: String,
    /// Target architecture (kernel-arch name, e.g. x86_64 / aarch64).
    #[arg(long, default_value = "x86_64")]
    pub arch: String,
    /// online: pull from mirror.openshift.com (registry optional).
    /// offline: build from the local cache / local rspacefs (registry optional).
    #[arg(long, value_enum, default_value_t = Mode::Online)]
    pub mode: Mode,
    /// Optional qregistry base URL (e.g. http://qregistry.gt.lo:5000). Never
    /// required to build an ISO/PXE; only the `registry` subcommand needs it.
    #[arg(long)]
    pub registry: Option<String>,
    /// Local raw-download cache directory.
    #[arg(long, default_value = "./cache")]
    pub cache: PathBuf,
}

#[derive(Args)]
pub struct OutArgs {
    #[command(flatten)]
    pub source: SourceArgs,
    /// Output path (file or directory, depending on the subcommand).
    #[arg(long)]
    pub out: PathBuf,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, ValueEnum)]
pub enum Mode {
    /// Pull artifacts from the upstream OpenShift mirror. No registry required.
    Online,
    /// Build from the local cache / local rspacefs; never touch the mirror.
    /// A central registry is optional — if --registry is set, missing
    /// artifacts are pulled from it; otherwise everything must be local.
    Offline,
}

pub fn run(cli: Cli) -> Result<()> {
    match cli.command {
        Command::Latest(a) => {
            println!("{}", crate::mirror::find_latest(&a.series, &a.arch)?);
            Ok(())
        }
        Command::Registry(src) => {
            let staged = crate::stage::stage(&src)?;
            crate::output::registry(&src, &staged)
        }
        Command::Iso(o) => {
            let staged = crate::stage::stage(&o.source)?;
            crate::output::iso(&o.source, &staged, &o.out)
        }
        Command::Pxe(o) => {
            let staged = crate::stage::stage(&o.source)?;
            crate::output::pxe(&o.source, &staged, &o.out)
        }
        Command::Raw(o) => {
            let staged = crate::stage::stage(&o.source)?;
            crate::output::raw(&o.source, &staged, &o.out)
        }
        Command::Qcow2(o) => {
            let staged = crate::stage::stage(&o.source)?;
            crate::output::qcow2(&o.source, &staged, &o.out)
        }
        Command::Files(o) => {
            let staged = crate::stage::stage(&o.source)?;
            crate::output::files(&o.source, &staged, &o.out)
        }
    }
}
