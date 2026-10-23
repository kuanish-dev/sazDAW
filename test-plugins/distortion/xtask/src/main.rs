//! The stock `nih_plug_xtask::main()` climbs to the *leftmost* ancestor containing a Cargo.toml,
//! which here would be the DAW root workspace. This plugin is a standalone workspace, so we
//! chdir into it ourselves and call the lower-level build/bundle functions.
use std::io;
use std::path::PathBuf;

const USAGE: &str = "usage: cargo xtask bundle <package> [cargo build args, e.g. --release]";

fn main() -> nih_plug_xtask::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .ok_or_else(|| io::Error::other("the xtask folder is not inside the plugin folder"))?
        .to_path_buf();
    std::env::set_current_dir(&root)?;
    let mut args = std::env::args().skip(1);
    if args.next().as_deref() != Some("bundle") {
        return Err(io::Error::other(USAGE).into());
    }
    let rest: Vec<String> = args.collect();
    let (packages, flags): (Vec<String>, Vec<String>) = rest.into_iter().partition(|a| !a.starts_with('-'));
    let target_dir = root.join("target");
    nih_plug_xtask::build(&packages, &flags)?;
    for p in &packages {
        nih_plug_xtask::bundle(&target_dir, p, &flags, false)?;
    }
    Ok(())
}
