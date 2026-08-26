use std::{sync::mpsc, thread};

use chip8_engine::CompatibilityProfile;
use serde::Deserialize;

const USER_AGENT: &str = "chip8-native-gui ROM library";

/// A public, versioned collection of directly playable CHIP-8 ROM files.
#[derive(Clone, Copy)]
struct Source {
    name: &'static str,
    repository: &'static str,
    revision: &'static str,
    path_prefix: &'static str,
}

const SOURCES: [Source; 2] = [
    Source {
        name: "CHIP-8 Archive — John Earnest",
        repository: "JohnEarnest/chip8Archive",
        revision: "master",
        path_prefix: "roms/",
    },
    Source {
        name: "dmatlack/chip8",
        repository: "dmatlack/chip8",
        revision: "master",
        path_prefix: "roms/",
    },
];

#[derive(Clone, Debug)]
pub struct Game {
    pub name: String,
    pub source: String,
    pub download_url: String,
    pub profile: CompatibilityProfile,
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
        .map(|entry| Game {
            name: game_name(&entry.path),
            source: source.name.into(),
            download_url: format!(
                "https://raw.githubusercontent.com/{}/{}/{}",
                source.repository, source.revision, entry.path
            ),
            profile: profile_for(&entry.path),
        })
        .collect())
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

fn profile_for(path: &str) -> CompatibilityProfile {
    if path.contains("/hires/") {
        CompatibilityProfile::SuperChip
    } else {
        CompatibilityProfile::OriginalChip8
    }
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
            profile_for("roms/hires/Trip8.ch8"),
            CompatibilityProfile::SuperChip
        );
    }
}
