//! parkwreck-plugin — proven crate name from the Aug 2026 tree.
//! Plugin loading, v1: manifest-driven, in-process.
//! A plugin is a directory containing `plugin.json` (an organelle manifest
//! with schema psw-organelle-manifest/v1) plus a factory registered under
//! the manifest's entrypoint key. Dynamic cdylib loading is an explicit
//! future transport, NOT v1 (documented boundary: no untrusted native code
//! in the rebuild until the Quarantine subsystem can contain it).
use pw_manifest::OrganelleManifest;
use pw_organelles::{Organelle, RegistryError};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum PluginError {
    #[error("io: {0}")]
    Io(String),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("no factory registered for entrypoint {0:?}")]
    NoFactory(String),
    #[error("registry: {0}")]
    Registry(String),
}

/// A discovered plugin: validated manifest + source directory.
#[derive(Debug, Clone)]
pub struct Plugin {
    pub manifest: OrganelleManifest,
    pub dir: PathBuf,
}

pub type Factory = Arc<dyn Fn() -> Arc<dyn Organelle> + Send + Sync>;

/// Discovers plugins under a root dir and instantiates them via factories.
#[derive(Default)]
pub struct PluginLoader {
    factories: HashMap<String, Factory>,
}

impl PluginLoader {
    pub fn new() -> Self {
        PluginLoader::default()
    }

    pub fn register_factory(&mut self, entrypoint: impl Into<String>, f: Factory) {
        self.factories.insert(entrypoint.into(), f);
    }

    /// Scan `<root>/*/plugin.json`. Fail-closed: invalid manifests are
    /// reported and skipped; the scan itself never panics.
    pub fn discover(&self, root: &Path) -> Result<Vec<Plugin>, PluginError> {
        let mut out = Vec::new();
        let rd = std::fs::read_dir(root).map_err(|e| PluginError::Io(e.to_string()))?;
        let mut dirs: Vec<PathBuf> = rd.filter_map(|e| e.ok()).map(|e| e.path()).filter(|p| p.is_dir()).collect();
        dirs.sort();
        for d in dirs {
            let pj = d.join("plugin.json");
            if !pj.exists() {
                continue;
            }
            let s = std::fs::read_to_string(&pj).map_err(|e| PluginError::Io(e.to_string()))?;
            let m = OrganelleManifest::from_json(&s)
                .map_err(|e| PluginError::Manifest(format!("{}: {e}", pj.display())))?;
            m.validate().map_err(|e| PluginError::Manifest(format!("{}: {e}", pj.display())))?;
            out.push(Plugin { manifest: m, dir: d });
        }
        Ok(out)
    }

    /// Instantiate a discovered plugin via its factory.
    pub fn instantiate(&self, plugin: &Plugin) -> Result<Arc<dyn Organelle>, PluginError> {
        let f = self.factories.get(&plugin.manifest.entrypoint)
            .ok_or_else(|| PluginError::NoFactory(plugin.manifest.entrypoint.clone()))?;
        let o = f();
        if o.name() != plugin.manifest.name {
            return Err(PluginError::Manifest(format!(
                "factory produced {:?}, manifest says {:?}",
                o.name(), plugin.manifest.name
            )));
        }
        Ok(o)
    }

    pub fn load_all(&self, root: &Path) -> Result<Vec<Arc<dyn Organelle>>, PluginError> {
        self.discover(root)?.iter().map(|p| self.instantiate(p)).collect()
    }
}

impl From<RegistryError> for PluginError {
    fn from(e: RegistryError) -> Self {
        PluginError::Registry(e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pw_manifest::{sample_manifest, Transport};
    use pw_organelles::OrganelleError;
    use serde_json::Value;

    struct Plug;
    impl Organelle for Plug {
        fn name(&self) -> &'static str { "psw-plug" }
        fn manifest(&self) -> OrganelleManifest {
            let mut m = sample_manifest("psw-plug");
            m.entrypoint = "plug::factory".into();
            m.transports = vec![Transport::InProcess];
            m
        }
        fn call(&self, c: &str, _i: Value) -> Result<Value, OrganelleError> {
            match c {
                "ping" => Ok(serde_json::json!({"pong": true})),
                _ => Err(OrganelleError::UnknownContract(c.into(), self.name().into())),
            }
        }
    }

    fn plug_dir(tag: &str, manifest: &OrganelleManifest) -> PathBuf {
        let root = std::env::temp_dir().join(format!("psw-plugin-{tag}"));
        let d = root.join("myplug");
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(d.join("plugin.json"), serde_json::to_string(manifest).unwrap()).unwrap();
        root
    }

    #[test]
    fn discover_finds_plugin() {
        let mut m = sample_manifest("psw-plug");
        m.entrypoint = "plug::factory".into();
        let root = plug_dir("t1", &m);
        let loader = PluginLoader::new();
        let found = loader.discover(&root).unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].manifest.name, "psw-plug");
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn discover_skips_invalid_manifest() {
        let mut m = sample_manifest("psw-plug");
        m.contracts.clear();
        let root = plug_dir("t2", &m);
        let loader = PluginLoader::new();
        assert!(loader.discover(&root).is_err());
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn discover_ignores_dirs_without_plugin_json() {
        let root = std::env::temp_dir().join("psw-plugin-t3");
        std::fs::create_dir_all(root.join("empty")).unwrap();
        let loader = PluginLoader::new();
        assert_eq!(loader.discover(&root).unwrap().len(), 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn instantiate_via_factory() {
        let mut m = sample_manifest("psw-plug");
        m.entrypoint = "plug::factory".into();
        let root = plug_dir("t4", &m);
        let mut loader = PluginLoader::new();
        loader.register_factory("plug::factory", Arc::new(|| Arc::new(Plug)));
        let organelles = loader.load_all(&root).unwrap();
        assert_eq!(organelles.len(), 1);
        assert_eq!(organelles[0].call("ping", Value::Null).unwrap()["pong"], true);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn instantiate_without_factory_fails() {
        let mut m = sample_manifest("psw-plug");
        m.entrypoint = "plug::factory".into();
        let root = plug_dir("t5", &m);
        let loader = PluginLoader::new();
        let found = loader.discover(&root).unwrap();
        assert!(matches!(loader.instantiate(&found[0]), Err(PluginError::NoFactory(_))));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn factory_name_mismatch_rejected() {
        struct Other;
        impl Organelle for Other {
            fn name(&self) -> &'static str { "psw-other" }
            fn manifest(&self) -> OrganelleManifest { sample_manifest("psw-other") }
            fn call(&self, _c: &str, _i: Value) -> Result<Value, OrganelleError> { Ok(Value::Null) }
        }
        let mut m = sample_manifest("psw-plug");
        m.entrypoint = "plug::factory".into();
        let root = plug_dir("t6", &m);
        let mut loader = PluginLoader::new();
        loader.register_factory("plug::factory", Arc::new(|| Arc::new(Other)));
        let found = loader.discover(&root).unwrap();
        assert!(matches!(loader.instantiate(&found[0]), Err(PluginError::Manifest(_))));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
