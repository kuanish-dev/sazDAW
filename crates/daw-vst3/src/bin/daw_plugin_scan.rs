//! `daw-plugin-scan`: the plugin scan helper. Loads ONE VST3 bundle in its own process and writes the result to an answer file,
//! so that a plugin crashing or hanging while loading does not bring the app down. The protocol and the app side are in
//! `daw_vst3::isolated` (`IsolatedScan`). Usage: `daw-plugin-scan <bundle.vst3> <answer file.json>`.

fn main() -> std::process::ExitCode {
    daw_vst3::helper_main(std::env::args_os().skip(1))
}
