use std::{collections::HashMap, sync::mpsc, thread};

use chip8_engine::CompatibilityProfile;
use serde::Deserialize;

const USER_AGENT: &str = "chip8-native-gui ROM library";

pub type Palette = [[u8; 3]; 4];

/// A public, versioned collection of directly playable CHIP-8 ROM files.
#[derive(Clone, Copy)]
struct Source {
    name: &'static str,
    repository: &'static str,
    revision: &'static str,
    path_prefix: &'static str,
    catalogue_path: Option<&'static str>,
}

const SOURCES: [Source; 2] = [
    Source {
        name: "CHIP-8 Archive — John Earnest",
        repository: "JohnEarnest/chip8Archive",
        revision: "master",
        path_prefix: "roms/",
        catalogue_path: Some("programs.json"),
    },
    Source {
        name: "dmatlack/chip8",
        repository: "dmatlack/chip8",
        revision: "master",
        path_prefix: "roms/",
        catalogue_path: None,
    },
];

#[derive(Clone, Debug)]
pub struct Game {
    pub name: String,
    pub source: String,
    pub download_url: String,
    pub profile: CompatibilityProfile,
    pub palette: Option<Palette>,
}

pub enum Update {
    Loaded(Vec<Game>),
    Downloaded { game: Game, bytes: Vec<u8> },
    Failed(String),
}

pub struct RomLibrary {
    sender: mpsc::Sender<Update>,
    receiver: mpsc::Receiver<Update>,
    pub games: Vec<Game>,
    pub status: String,
    pub filter: String,
    pub profile_filter: Option<CompatibilityProfile>,
}

impl RomLibrary {
    pub fn load() -> Self {
        let (sender, receiver) = mpsc::channel();
        let load_sender = sender.clone();
        thread::spawn(move || match fetch_games() {
            Ok(games) => {
                let _ = load_sender.send(Update::Loaded(games));
            }
            Err(error) => {
                let _ = load_sender.send(Update::Failed(error));
            }
        });
        Self {
            sender,
            receiver,
            games: Vec::new(),
            status: "Chargement des bibliothèques…".into(),
            filter: String::new(),
            profile_filter: None,
        }
    }

    pub fn receive_updates(&mut self) -> Option<Update> {
        let mut download = None;
        while let Ok(update) = self.receiver.try_recv() {
            match update {
                Update::Loaded(games) => {
                    self.status = format!("{} jeux disponibles", games.len());
                    self.games = games;
                }
                Update::Failed(error) => self.status = error,
                Update::Downloaded { .. } => download = Some(update),
            }
        }
        download
    }

    pub fn download(&mut self, game: Game) {
        self.status = format!("Téléchargement de {}…", game.name);
        let download_sender = self.sender.clone();
        thread::spawn(move || match download_rom(&game) {
            Ok(bytes) => {
                let _ = download_sender.send(Update::Downloaded { game, bytes });
            }
            Err(error) => {
                let _ = download_sender.send(Update::Failed(error));
            }
        });
    }
}

#[derive(Deserialize)]
struct TreeResponse {
    tree: Vec<TreeEntry>,
}

#[derive(Deserialize)]
struct TreeEntry {
    path: String,
    #[serde(rename = "type")]
    kind: String,
}

#[derive(Deserialize)]
struct CatalogueEntry {
    platform: Option<String>,
    #[serde(default)]
    options: CatalogueOptions,
}

#[derive(Default, Deserialize)]
struct CatalogueOptions {
    #[serde(rename = "backgroundColor")]
    background: Option<String>,
    #[serde(rename = "fillColor")]
    fill: Option<String>,
    #[serde(rename = "fillColor2")]
    fill_second_plane: Option<String>,
    #[serde(rename = "blendColor")]
    blend: Option<String>,
}

fn fetch_games() -> Result<Vec<Game>, String> {
    let client = client()?;
    let mut games = Vec::new();
    let mut errors = Vec::new();
    for source in SOURCES {
        match fetch_source(&client, source) {
            Ok(mut source_games) => games.append(&mut source_games),
            Err(error) => errors.push(format!("{} : {error}", source.name)),
        }
    }
    if games.is_empty() {
        return Err(format!(
            "Impossible de charger les bibliothèques. {}",
            errors.join(" — ")
        ));
    }
    games.sort_by(|left, right| {
        left.name
            .cmp(&right.name)
            .then(left.source.cmp(&right.source))
    });
    Ok(games)
}

fn fetch_source(client: &reqwest::blocking::Client, source: Source) -> Result<Vec<Game>, String> {
    let catalogue = source
        .catalogue_path
        .and_then(|path| fetch_catalogue(client, source, path).ok())
        .unwrap_or_default();
    let url = format!(
        "https://api.github.com/repos/{}/git/trees/{}?recursive=1",
        source.repository, source.revision
    );
    let tree: TreeResponse = client
        .get(url)
        .send()
        .map_err(request_error)?
        .error_for_status()
        .map_err(request_error)?
        .json()
        .map_err(request_error)?;
    Ok(tree
        .tree
        .into_iter()
        .filter(|entry| entry.kind == "blob" && entry.path.starts_with(source.path_prefix))
        .filter(|entry| entry.path.to_ascii_lowercase().ends_with(".ch8"))
        .map(|entry| {
            let profile = profile_for(&entry.path, &catalogue);
            let identifier = game_name(&entry.path);
            let palette = (profile == CompatibilityProfile::XoChip)
                .then(|| catalogue.get(&identifier).and_then(palette_for))
                .flatten();
            Game {
                name: identifier,
                source: source.name.into(),
                download_url: format!(
                    "https://raw.githubusercontent.com/{}/{}/{}",
                    source.repository, source.revision, entry.path
                ),
                profile,
                palette,
            }
        })
        .collect())
}

fn fetch_catalogue(
    client: &reqwest::blocking::Client,
    source: Source,
    path: &str,
) -> Result<HashMap<String, CatalogueEntry>, String> {
    let url = format!(
        "https://raw.githubusercontent.com/{}/{}/{}",
        source.repository, source.revision, path
    );
    client
        .get(url)
        .send()
        .map_err(request_error)?
        .error_for_status()
        .map_err(request_error)?
        .json()
        .map_err(request_error)
}

fn download_rom(game: &Game) -> Result<Vec<u8>, String> {
    let bytes = client()?
        .get(&game.download_url)
        .send()
        .map_err(request_error)?
        .error_for_status()
        .map_err(request_error)?
        .bytes()
        .map_err(request_error)?
        .to_vec();
    if bytes.is_empty() {
        return Err(format!("{} est vide", game.name));
    }
    Ok(bytes)
}

fn client() -> Result<reqwest::blocking::Client, String> {
    reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .build()
        .map_err(request_error)
}

fn request_error(error: reqwest::Error) -> String {
    format!("erreur réseau : {error}")
}

fn game_name(path: &str) -> String {
    path.rsplit('/')
        .next()
        .unwrap_or(path)
        .trim_end_matches(".ch8")
        .to_owned()
}

fn profile_for(path: &str, catalogue: &HashMap<String, CatalogueEntry>) -> CompatibilityProfile {
    let identifier = game_name(path);
    if let Some(platform) = catalogue
        .get(&identifier)
        .and_then(|entry| entry.platform.as_deref())
    {
        return profile_for_platform(platform);
    }
    if path.contains("/hires/") {
        CompatibilityProfile::SuperChip
    } else {
        CompatibilityProfile::OriginalChip8
    }
}

fn profile_for_platform(platform: &str) -> CompatibilityProfile {
    match platform.to_ascii_lowercase().as_str() {
        "chip48" => CompatibilityProfile::Chip48,
        "schip" | "superchip" => CompatibilityProfile::SuperChip,
        "xo-chip" | "xochip" => CompatibilityProfile::XoChip,
        _ => CompatibilityProfile::OriginalChip8,
    }
}

fn palette_for(entry: &CatalogueEntry) -> Option<Palette> {
    Some([
        parse_colour(entry.options.background.as_deref()?)?,
        parse_colour(entry.options.fill.as_deref()?)?,
        parse_colour(entry.options.fill_second_plane.as_deref()?)?,
        parse_colour(entry.options.blend.as_deref()?)?,
    ])
}

fn parse_colour(value: &str) -> Option<[u8; 3]> {
    let hex = value.strip_prefix('#')?;
    if hex.len() != 6 {
        return None;
    }
    Some([
        u8::from_str_radix(&hex[0..2], 16).ok()?,
        u8::from_str_radix(&hex[2..4], 16).ok()?,
        u8::from_str_radix(&hex[4..6], 16).ok()?,
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derives_a_human_readable_name_from_a_rom_path() {
        assert_eq!(game_name("roms/games/Space Invaders.ch8"), "Space Invaders");
    }

    #[test]
    fn hires_roms_use_the_super_chip_profile() {
        assert_eq!(
            profile_for("roms/hires/Trip8.ch8", &HashMap::new()),
            CompatibilityProfile::SuperChip
        );
    }

    #[test]
    fn maps_catalogue_platforms_to_emulation_profiles() {
        assert_eq!(
            profile_for_platform("schip"),
            CompatibilityProfile::SuperChip
        );
        assert_eq!(profile_for_platform("xochip"), CompatibilityProfile::XoChip);
    }

    #[test]
    fn reads_the_four_octo_palette_colours() {
        let entry = CatalogueEntry {
            platform: Some("xochip".into()),
            options: CatalogueOptions {
                background: Some("#000000".into()),
                fill: Some("#112233".into()),
                fill_second_plane: Some("#445566".into()),
                blend: Some("#778899".into()),
            },
        };
        assert_eq!(
            palette_for(&entry),
            Some([[0, 0, 0], [17, 34, 51], [68, 85, 102], [119, 136, 153]])
        );
    }
}
