//! A loaded `.vst3` bundle: the dynamic library and its class factory (`IPluginFactory`).
//!
//! The entity's job is to keep the bundle loaded exactly as long as it is needed and unload it correctly.
//! On macOS the VST3 spec order is: open the library from `Contents/MacOS`, call
//! `bundleEntry(CFBundleRef)`, get the factory via `GetPluginFactory`; at the end release the factory,
//! call `bundleExit` and only then close the library.

use std::mem::ManuallyDrop;
use std::path::{Path, PathBuf};

use libloading::Library;
use vst3::ComPtr;
use vst3::Steinberg::IPluginFactory;

pub struct Bundle {
    path: PathBuf,
    /// Released by hand in `Drop` strictly before `bundleExit`.
    factory: ManuallyDrop<ComPtr<IPluginFactory>>,
    exit: Option<ExitFn>,
    #[cfg(target_os = "macos")]
    _cf_bundle: core_foundation::bundle::CFBundle,
    _library: Library,
}

type ExitFn = unsafe extern "C" fn() -> bool;

// SAFETY: by the VST3 spec the factory is thread-safe for creating classes, and in this crate calls to the factory
// come from one thread at a time anyway: a shared bundle is held by `PluginSource` under its `Mutex`, which creates
// instances (`PluginInstance::create`) without releasing the lock; `Vst3Scanner` owns its bundle alone in one thread.
// The library and the CFBundle are just handles that can be passed between threads.
unsafe impl Send for Bundle {}
// SAFETY: see above; `Bundle` exposes no shared mutable state.
unsafe impl Sync for Bundle {}

impl Bundle {
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn factory(&self) -> &ComPtr<IPluginFactory> {
        &self.factory
    }

    #[cfg(target_os = "macos")]
    pub fn load(path: &Path) -> Result<Self, String> {
        use core_foundation::base::TCFType;
        use core_foundation::bundle::{CFBundle, CFBundleRef};
        use core_foundation::url::CFURL;

        let url = CFURL::from_path(path, true).ok_or("the path is not usable as a CFURL")?;
        let cf_bundle = CFBundle::new(url).ok_or("could not open as a CFBundle (no Info.plist?)")?;
        let executable = cf_bundle.executable_url().and_then(|url| url.to_path()).ok_or("Info.plist names no executable")?;
        // SAFETY: loading a foreign library runs its initializers; this is the unavoidable essence of hosting plugins.
        let library = unsafe { Library::new(&executable) }.map_err(|e| format!("the library did not load: {e}"))?;

        type EntryFn = unsafe extern "C" fn(CFBundleRef) -> bool;
        // SAFETY: the symbol type matches the `bundleEntry` signature from the VST3 SDK (macOS).
        let entry: EntryFn = *unsafe { library.get::<EntryFn>(b"bundleEntry\0") }.map_err(|e| format!("no bundleEntry export: {e}"))?;
        // SAFETY: the symbol type matches the `bundleExit` signature from the VST3 SDK (macOS).
        let exit: ExitFn = *unsafe { library.get::<ExitFn>(b"bundleExit\0") }.map_err(|e| format!("no bundleExit export: {e}"))?;
        // SAFETY: `cf_bundle` is alive, and `bundleEntry` must be called first and once.
        if !unsafe { entry(cf_bundle.as_concrete_TypeRef()) } {
            return Err("bundleEntry refused".into());
        }

        let factory = match Self::factory_from(&library) {
            Ok(factory) => factory,
            Err(error) => {
                // SAFETY: `bundleEntry` was called successfully above, the library is still loaded.
                unsafe { exit() };
                return Err(error);
            }
        };
        Ok(Self {
            path: path.to_path_buf(),
            factory: ManuallyDrop::new(factory),
            exit: Some(exit),
            _cf_bundle: cf_bundle,
            _library: library,
        })
    }

    #[cfg(not(target_os = "macos"))]
    pub fn load(_path: &Path) -> Result<Self, String> {
        Err("VST3 hosting is implemented only for macOS".into())
    }

    #[cfg(target_os = "macos")]
    fn factory_from(library: &Library) -> Result<ComPtr<IPluginFactory>, String> {
        type FactoryFn = unsafe extern "C" fn() -> *mut IPluginFactory;
        // SAFETY: the symbol type matches the `GetPluginFactory` signature from the VST3 SDK.
        let get_factory: FactoryFn =
            *unsafe { library.get::<FactoryFn>(b"GetPluginFactory\0") }.map_err(|e| format!("no GetPluginFactory export: {e}"))?;
        // SAFETY: `GetPluginFactory` takes no arguments; `bundleEntry` was already called, the library is loaded.
        let raw = unsafe { get_factory() };
        // SAFETY: the plugin function returns the factory with a reference already counted, which `ComPtr` now owns.
        unsafe { ComPtr::from_raw(raw) }.ok_or_else(|| "GetPluginFactory returned a null pointer".to_string())
    }
}

impl Drop for Bundle {
    fn drop(&mut self) {
        // SAFETY: the factory is released exactly once and before `bundleExit`, as the spec requires;
        // after `Drop::drop` the field is never read again.
        unsafe { ManuallyDrop::drop(&mut self.factory) };
        if let Some(exit) = self.exit.take() {
            // SAFETY: `bundleEntry` was called on load, the library is not closed yet (the `_library` field below).
            unsafe { exit() };
        }
    }
}
