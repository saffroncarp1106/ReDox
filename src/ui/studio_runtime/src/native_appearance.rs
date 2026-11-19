use crate::document::Document;
use crate::scenes::{SceneId, Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

pub const TRANSCRIPT_SURFACE_ID: &str = "codex/conversation/transcript.surface";
pub const DEFAULT_TRANSCRIPT_SURFACE: [u8; 3] = [24, 24, 24];
pub const SETTINGS_NAVIGATION_ID: &str = "codex/settings/navigation.surface";
pub const DEFAULT_SETTINGS_NAVIGATION: [u8; 3] = [32, 32, 32];
const VERSION: u8 = 1;
const MAX_BYTES: u64 = 4096;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ThemeColor {
    Background,
    Sidebar,
    Rail,
    Surface,
    InputBackground,
    Text,
    MenuText,
    Muted,
    Border,
    Accent,
}

impl ThemeColor {
    pub const ALL: [Self; 10] = [
        Self::Background,
        Self::Sidebar,
        Self::Rail,
        Self::Surface,
        Self::InputBackground,
        Self::Text,
        Self::MenuText,
        Self::Muted,
        Self::Border,
        Self::Accent,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Background => "Background",
            Self::Sidebar => "Sidebar",
            Self::Rail => "Navigation rail",
            Self::Surface => "Surface",
            Self::InputBackground => "Input background",
            Self::Text => "Text",
            Self::MenuText => "Menu text",
            Self::Muted => "Secondary text",
            Self::Border => "Border",
            Self::Accent => "Accent",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NativeRegionId {
    TitleBar,
    Sidebar,
    Conversation,
    Composer,
    SettingsNavigation,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRegion {
    pub id: NativeRegionId,
    /// Viewport coordinates in logical points: x, y, width, height.
    pub bounds: [f32; 4],
    pub color: ThemeColor,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeDesign {
    pub palette: BTreeMap<ThemeColor, [u8; 3]>,
    pub regions: Vec<NativeRegion>,
}

impl NativeDesign {
    pub(crate) fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        for region in &self.regions {
            if !ids.insert(region.id)
                || region
                    .bounds
                    .iter()
                    .any(|value| !value.is_finite() || value.abs() > 32000.)
                || region.bounds[2] <= 0.
                || region.bounds[3] <= 0.
            {
                return Err("Invalid native design region".into());
            }
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeAppearance {
    version: u8,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    theme_palette: BTreeMap<String, BTreeMap<ThemeColor, [u8; 3]>>,
    #[serde(
        default,
        rename = "codex/conversation/transcript.surface",
        skip_serializing_if = "Option::is_none"
    )]
    pub transcript_surface: Option<[u8; 3]>,
    #[serde(
        default,
        rename = "codex/settings/navigation.surface",
        skip_serializing_if = "Option::is_none"
    )]
    pub settings_navigation: Option<[u8; 3]>,
}

impl Default for NativeAppearance {
    fn default() -> Self {
        Self {
            version: VERSION,
            theme_palette: BTreeMap::new(),
            transcript_surface: None,
            settings_navigation: None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NativeAppearanceProperty {
    TranscriptSurface,
    SettingsNavigation,
}

impl NativeAppearanceProperty {
    pub fn from_source(source: &Source) -> Option<Self> {
        if source.theme != "codex" {
            return None;
        }
        match source.scene {
            SceneId::Conversation => Some(Self::TranscriptSurface),
            SceneId::Account
            | SceneId::Models
            | SceneId::Workspace
            | SceneId::Appearance
            | SceneId::Cursors
            | SceneId::Advanced
            | SceneId::Updates
            | SceneId::About => Some(Self::SettingsNavigation),
            _ => None,
        }
    }

    pub fn id(self) -> &'static str {
        match self {
            Self::TranscriptSurface => TRANSCRIPT_SURFACE_ID,
            Self::SettingsNavigation => SETTINGS_NAVIGATION_ID,
        }
    }

    pub fn default_color(self) -> [u8; 3] {
        match self {
            Self::TranscriptSurface => DEFAULT_TRANSCRIPT_SURFACE,
            Self::SettingsNavigation => DEFAULT_SETTINGS_NAVIGATION,
        }
    }
}

impl NativeAppearance {
    pub fn theme_color(&self, theme: &str, token: ThemeColor) -> Option<[u8; 3]> {
        self.theme_palette.get(theme)?.get(&token).copied()
    }

    pub fn set_theme_color(
        &mut self,
        theme: &str,
        token: ThemeColor,
        color: Option<[u8; 3]>,
    ) -> bool {
        if !crate::scenes::THEMES.contains(&theme) {
            return false;
        }
        if let Some(color) = color {
            self.theme_palette
                .entry(theme.into())
                .or_default()
                .insert(token, color);
        } else if let Some(palette) = self.theme_palette.get_mut(theme) {
            palette.remove(&token);
            if palette.is_empty() {
                self.theme_palette.remove(theme);
            }
        }
        true
    }

    pub fn color(&self, property: NativeAppearanceProperty) -> Option<[u8; 3]> {
        match property {
            NativeAppearanceProperty::TranscriptSurface => self.transcript_surface,
            NativeAppearanceProperty::SettingsNavigation => self.settings_navigation,
        }
    }

    pub fn set_color(&mut self, property: NativeAppearanceProperty, color: Option<[u8; 3]>) {
        match property {
            NativeAppearanceProperty::TranscriptSurface => self.transcript_surface = color,
            NativeAppearanceProperty::SettingsNavigation => self.settings_navigation = color,
        }
    }

    pub fn apply_to_source(&self, document: &mut Document) {
        let Some(property) = document
            .source
            .as_ref()
            .and_then(NativeAppearanceProperty::from_source)
        else {
            return;
        };
        let source_color = document
            .native_design
            .as_ref()
            .and_then(|design| {
                design.palette.get(&match property {
                    NativeAppearanceProperty::TranscriptSurface => ThemeColor::Background,
                    NativeAppearanceProperty::SettingsNavigation => ThemeColor::Rail,
                })
            })
            .copied();
        let color = self
            .color(property)
            .or(source_color)
            .unwrap_or_else(|| property.default_color());
        for node in document.pages.iter_mut().flat_map(|page| &mut page.nodes) {
            if node.name != property.id() {
                continue;
            }
            if let Some(art) = &mut node.artwork {
                for vertex in art.meshes.iter_mut().flat_map(|mesh| &mut mesh.vertices) {
                    if vertex.color[3] != 0 {
                        vertex.color[..3].copy_from_slice(&color);
                    }
                }
            }
        }
    }

    pub fn path() -> PathBuf {
        std::env::var_os("LOCALAPPDATA")
            .map(PathBuf::from)
            .or_else(|| {
                std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share"))
            })
            .unwrap_or_else(std::env::temp_dir)
            .join("ReDox/Studio/native-appearance.json")
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let mut bytes = Vec::new();
        File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Native appearance file exceeds 4 KiB",
            ));
        }
        let appearance: Self = serde_json::from_slice(&bytes)?;
        if appearance.version != VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unsupported native appearance version",
            ));
        }
        if appearance
            .theme_palette
            .keys()
            .any(|theme| !crate::scenes::THEMES.contains(&theme.as_str()))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Unknown native appearance theme",
            ));
        }
        Ok(appearance)
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let parent = path.parent().filter(|dir| !dir.as_os_str().is_empty());
        let parent = parent.unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let mut temp = tempfile::NamedTempFile::new_in(parent)?;
        temp.write_all(&serde_json::to_vec_pretty(self)?)?;
        temp.as_file().sync_all()?;
        temp.persist(path)?;
        Ok(())
    }
}
