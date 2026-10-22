//! Data-only native panels. Packs never define values, permissions, RPCs or code.
use crate::{model::{Device, Property}, storage};
use serde::{Deserialize, Serialize};
use std::{collections::HashSet, io::Read, path::Path};

const MAX_BYTES: usize = 64 * 1024;
const MAX_LOCAL_PACKS: usize = 32;
const BUILTINS: &str = include_str!("../../adapters/builtin.json");

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pack {
    pub schema_version: u32,
    pub id: String,
    pub version: u32,
    pub name: String,
    pub license: String,
    pub source: String,
    pub matches: Matches,
    pub sections: Vec<Section>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Matches {
    #[serde(default)]
    pub models: Vec<String>,
    #[serde(default)]
    pub spec_urns: Vec<String>,
    #[serde(default)]
    pub device_types: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Page { Controls, Settings }

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Style { Controls, Readings }

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Section {
    pub page: Page,
    pub title: String,
    pub style: Style,
    pub properties: Vec<Selector>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Selector {
    pub key: String,
    #[serde(default)]
    pub service_key: Option<String>,
}

fn short_text(text: &str, max: usize) -> bool {
    !text.trim().is_empty() && text.chars().count() <= max && !text.chars().any(char::is_control)
}
fn token(text: &str) -> bool {
    !text.is_empty() && text.len() <= 96 && text.bytes().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-')
}
fn model_name(text: &str) -> bool {
    text.contains('.') && text.len() <= 160 && text.bytes().all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
}

impl Pack {
    pub fn parse(bytes: &[u8]) -> Result<Self, String> {
        if bytes.len() > MAX_BYTES { return Err("适配包超过 64 KiB 限制".into()); }
        let pack: Self = serde_json::from_slice(bytes).map_err(|_| "适配包 JSON 格式或字段不受支持")?;
        pack.validate()?;
        Ok(pack)
    }

    fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 || self.version == 0 { return Err("适配包版本不受支持".into()); }
        if !token(&self.id) || !short_text(&self.name, 40) || !short_text(&self.license, 80) || !short_text(&self.source, 240) {
            return Err("适配包标识、名称或来源无效".into());
        }
        let m = &self.matches;
        if m.models.len() + m.spec_urns.len() + m.device_types.len() == 0 ||
            m.models.len() + m.spec_urns.len() + m.device_types.len() > 64 ||
            m.models.iter().any(|s| !model_name(s)) ||
            m.spec_urns.iter().any(|s| !s.starts_with("urn:miot-spec-v2:device:") || !short_text(s, 240) || s.contains('*')) ||
            m.device_types.iter().any(|s| !token(s)) {
            return Err("适配包需要明确的型号或公开设备类型，不支持通配匹配".into());
        }
        if self.sections.is_empty() || self.sections.len() > 16 { return Err("适配包分组数量无效".into()); }
        for section in &self.sections {
            if !short_text(&section.title, 32) || section.properties.is_empty() || section.properties.len() > 32 ||
                section.properties.iter().any(|s| !token(&s.key) || s.service_key.as_ref().is_some_and(|k| !token(k))) {
                return Err("适配包控件分组无效".into());
            }
        }
        Ok(())
    }

    fn score(&self, device: &Device) -> u8 {
        if self.matches.models.contains(&device.model) { return 3; }
        if !device.spec_urn.is_empty() && self.matches.spec_urns.contains(&device.spec_urn) { return 2; }
        let kind = device.spec_urn.strip_prefix("urn:miot-spec-v2:device:").and_then(|s| s.split(':').next());
        if kind.is_some_and(|kind| self.matches.device_types.iter().any(|s| s == kind)) { 1 } else { 0 }
    }
}

pub struct Catalog { packs: Vec<Pack>, pub issues: usize }

impl Catalog {
    pub fn load(root: &Path) -> Self {
        let packs: Vec<Pack> = serde_json::from_str(BUILTINS).expect("validated built-in panel data");
        let mut catalog = Self { packs, issues: 0 };
        let directory = root.join("adapters");
        let entries = match std::fs::read_dir(&directory) {
            Ok(entries) => entries,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return catalog,
            Err(_) => { catalog.issues += 1; return catalog; }
        };
        let mut paths: Vec<_> = entries.filter_map(Result::ok).map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|s| s == "json")).collect();
        paths.sort();
        if paths.len() > MAX_LOCAL_PACKS { catalog.issues += paths.len() - MAX_LOCAL_PACKS; }
        for path in paths.iter().take(MAX_LOCAL_PACKS) {
            match read_pack(path).and_then(|pack| catalog.merge(pack)) {
                Ok(()) => {}, Err(_) => catalog.issues += 1,
            }
        }
        catalog
    }

    fn merge(&mut self, pack: Pack) -> Result<(), String> {
        if let Some(current) = self.packs.iter_mut().find(|p| p.id == pack.id) {
            if pack.version <= current.version { return Err("适配包版本必须高于已有版本".into()); }
            *current = pack;
        } else { self.packs.push(pack); }
        Ok(())
    }

    pub fn count(&self) -> usize { self.packs.len() }

    /// Equal-specificity competing packs fail closed to the generic panel.
    pub fn resolve(&self, device: &Device) -> Option<&Pack> {
        let best = self.packs.iter().map(|p| p.score(device)).max().unwrap_or(0);
        if best == 0 { return None; }
        let mut candidates = self.packs.iter().filter(|p| p.score(device) == best);
        let pack = candidates.next()?;
        candidates.next().is_none().then_some(pack)
    }

    pub fn install(&mut self, root: &Path, file: &Path) -> Result<String, String> {
        let pack = read_pack(file)?;
        if self.packs.iter().any(|p| p.id == pack.id && p.version >= pack.version) {
            return Err("已安装相同或更新版本；原适配包保留".into());
        }
        if self.packs.len() >= MAX_LOCAL_PACKS + 5 && !self.packs.iter().any(|p| p.id == pack.id) {
            return Err("本机适配包数量已达上限".into());
        }
        let name = pack.name.clone();
        let path = root.join("adapters").join(format!("{}.json", pack.id));
        let bytes = serde_json::to_vec_pretty(&pack).map_err(|_| "无法编码适配包")?;
        storage::atomic_write(&path, &bytes)?;
        self.merge(pack)?;
        Ok(name)
    }
}

fn read_pack(path: &Path) -> Result<Pack, String> {
    let file = std::fs::File::open(path).map_err(|_| "无法打开适配包文件")?;
    if !file.metadata().map_err(|_| "无法检查适配包文件")?.is_file() { return Err("请选择 JSON 文件".into()); }
    let mut bytes = Vec::new();
    file.take((MAX_BYTES + 1) as u64).read_to_end(&mut bytes).map_err(|_| "无法读取适配包文件")?;
    Pack::parse(&bytes)
}

pub struct BoundSection<'a> { pub section: &'a Section, pub properties: Vec<&'a Property> }

pub fn bind<'a>(pack: &'a Pack, device: &'a Device, page: Page) -> Vec<BoundSection<'a>> {
    let mut used = HashSet::new();
    pack.sections.iter().filter(|s| s.page == page).filter_map(|section| {
        let mut properties = Vec::new();
        for selector in &section.properties {
            for p in &device.properties {
                if selector.key != p.key || selector.service_key.as_ref().is_some_and(|key| key != &p.service_key) { continue; }
                let suitable = match section.style { Style::Controls => p.writable, Style::Readings => p.readable && !p.writable };
                if !suitable || (page == Page::Controls && crate::presentation::advanced_for(device, p)) ||
                    (page == Page::Settings && !crate::presentation::advanced_for(device, p)) { continue; }
                // Repeated or malformed SPEC targets must not become a new control.
                if device.properties.iter().filter(|q| q.siid == p.siid && q.piid == p.piid).count() != 1 { continue; }
                if used.insert((p.siid, p.piid)) { properties.push(p); }
            }
        }
        (!properties.is_empty()).then_some(BoundSection { section, properties })
    }).collect()
}

/// All real properties outside rendered sections remain in the generic UI.
pub fn remaining<'a>(device: &'a Device, page: Page, primary: Option<(u32,u32)>, used: &HashSet<(u32,u32)>) -> Vec<&'a Property> {
    device.properties.iter().filter(|p|
        Some((p.siid,p.piid)) != primary && !used.contains(&(p.siid,p.piid)) &&
        crate::presentation::advanced_for(device,p) == (page == Page::Settings)
    ).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    fn sample() -> Pack { serde_json::from_str::<Vec<Pack>>(BUILTINS).unwrap().remove(0) }
    fn device() -> Device { Device { model: "vendor.plug.v1".into(), spec_urn: "urn:miot-spec-v2:device:outlet:1".into(),
        kind: "plug".into(), properties: vec![
            Property { siid:2, piid:1, key:"on".into(), service_key:"outlet".into(), writable:true, readable:true, format:"bool".into(), value:Some(json!(false)), ..Default::default() },
            Property { siid:3, piid:1, key:"electric-power".into(), readable:true, unit:"watt".into(), value:Some(json!(15)), ..Default::default() },
            Property { siid:4, piid:1, key:"vendor-feature".into(), readable:true, ..Default::default() },
        ], ..Default::default() } }
    #[test]
    fn embedded_packs_validate_and_bound_controls_preserve_spec() {
        let catalog = Catalog::load(Path::new("nonexistent-panel-test-directory"));
        assert_eq!(catalog.count(), 5);
        for p in &catalog.packs { p.validate().unwrap(); }
        let d = device();
        let bound = bind(catalog.resolve(&d).unwrap(), &d, Page::Controls);
        let props: Vec<_> = bound.iter().flat_map(|s| &s.properties).collect();
        assert_eq!(props.len(),2);
        assert_eq!(props[0].value,Some(json!(false)));
        assert!(!props[1].writable);
        assert_eq!(props[1].unit,"watt");
        assert!(!props.iter().any(|p|p.key=="vendor-feature")); // caller renders leftovers
    }
    #[test]
    fn exact_model_wins_conflict_falls_back_and_names_never_match() {
        let mut catalog = Catalog { packs: vec![sample()], issues:0 };
        let mut exact=sample(); exact.id="exact-profile".into();exact.matches.device_types.clear();exact.matches.models=vec!["vendor.plug.v1".into()];
        catalog.merge(exact.clone()).unwrap();assert_eq!(catalog.resolve(&device()).unwrap().id,"exact-profile");
        exact.id="conflicting-profile".into();catalog.merge(exact).unwrap();assert!(catalog.resolve(&device()).is_none());
        let named=Device { name:"插座".into(),kind:"plug".into(),model:"vendor.sensor.v1".into(),..Default::default() };
        assert!(Catalog{packs:vec![sample()],issues:0}.resolve(&named).is_none());
    }
    #[test]
    fn malformed_code_fields_versions_and_traversal_are_rejected() {
        let valid=serde_json::to_value(sample()).unwrap();
        for (key,value) in [("schemaVersion",json!(2)),("id",json!("../escape")),("script",json!("execute()")),("version",json!(0))] {
            let mut v=valid.clone();v[key]=value;assert!(Pack::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        }
        let mut v=valid;v["matches"]["models"]=json!(["vendor.*"]);
        assert!(Pack::parse(&serde_json::to_vec(&v).unwrap()).is_err());
        assert!(Pack::parse(&vec![b' ';MAX_BYTES+1]).is_err());
    }
    #[test]
    fn bindings_deduplicate_and_do_not_promote_indicator_or_readonly_controls() {
        let mut d=device();d.properties.push(Property {siid:5,piid:1,key:"on".into(),service_key:"indicator-light".into(),writable:true,..Default::default()});
        let p=sample();let bound=bind(&p,&d,Page::Controls);
        assert_eq!(bound.iter().flat_map(|s|&s.properties).filter(|p|p.key=="on").count(),1);
        d.properties.push(d.properties[0].clone());
        assert!(!bind(&p,&d,Page::Controls).iter().flat_map(|s|&s.properties).any(|p|p.key=="on"));
    }
    #[test]
    fn install_roundtrip_and_invalid_upgrade_preserve_working_pack() {
        let root=std::env::temp_dir().join(format!("homepanel-panel-test-{}",rand::random::<u64>()));
        std::fs::create_dir_all(&root).unwrap();
        let source=root.join("input.json");let mut pack=sample();pack.id="custom-outlet".into();
        pack.matches.device_types.clear();pack.matches.models=vec!["vendor.plug.v1".into()];
        std::fs::write(&source,serde_json::to_vec(&pack).unwrap()).unwrap();
        let mut catalog=Catalog::load(&root);catalog.install(&root,&source).unwrap();
        let installed=root.join("adapters/custom-outlet.json");let original=std::fs::read(&installed).unwrap();
        assert!(catalog.install(&root,&source).is_err());
        assert_eq!(original,std::fs::read(&installed).unwrap());
        assert_eq!(Catalog::load(&root).resolve(&device()).unwrap().id,"custom-outlet");
        // Only these known files/directories were created by this test.
        for file in [source,installed] {std::fs::remove_file(file).unwrap();}
        std::fs::remove_dir(root.join("adapters")).unwrap();std::fs::remove_dir(root).unwrap();
    }
    #[test]
    fn example_exact_model_keeps_unmatched_features_and_classifies_settings() {
        let pack=Pack::parse(include_bytes!("../../adapters/examples/chuangmi-plug-212a01.json")).unwrap();
        let mut d=device();d.model="chuangmi.plug.212a01".into();
        d.properties.push(Property{siid:5,piid:1,key:"on".into(),service_key:"indicator-light".into(),writable:true,..Default::default()});
        assert_eq!(pack.score(&d),3);
        let controls=bind(&pack,&d,Page::Controls);let settings=bind(&pack,&d,Page::Settings);
        assert_eq!(controls.iter().flat_map(|s|&s.properties).count(),2);
        assert_eq!(settings.iter().flat_map(|s|&s.properties).count(),1);
        assert!(settings[0].properties[0].service_key=="indicator-light");
        let used=controls.iter().flat_map(|s|&s.properties).map(|p|(p.siid,p.piid)).collect();
        let fallback=remaining(&d,Page::Controls,None,&used);
        assert_eq!(fallback.iter().map(|p|p.key.as_str()).collect::<Vec<_>>(),vec!["vendor-feature"]);
        assert!(!fallback.iter().any(|p|p.service_key=="indicator-light"));
        d.model="chuangmi.plug.other".into();assert_eq!(pack.score(&d),0);
    }
}
