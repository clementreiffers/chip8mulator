use std::{collections::HashMap, fs, path::Path, sync::mpsc, thread};

use bollard::{
    Docker,
    container::{
        Config, CreateContainerOptions, LogsOptions, RemoveContainerOptions, StartContainerOptions,
        WaitContainerOptions,
    },
    image::CreateImageOptions,
    models::{HostConfig, Mount, MountTypeEnum},
};
use chip8_engine::CompatibilityProfile;
use futures_util::StreamExt;
use serde::Deserialize;

const USER_AGENT: &str = "chip8-native-gui ROM library";

pub type Palette = [[u8; 3]; 4];

/// A public, versioned collection of playable ROMs or Octo source files.
#[derive(Clone, Copy)]
struct Source {
    name: &'static str,
    repository: &'static str,
    revision: &'static str,
    path_prefix: &'static str,
    catalogue_path: Option<&'static str>,
    extension: &'static str,
}

const SOURCES: [Source; 3] = [
    Source {
        name: "CHIP-8 Archive — John Earnest",
        repository: "JohnEarnest/chip8Archive",
        revision: "master",
        path_prefix: "roms/",
        catalogue_path: Some("programs.json"),
        extension: ".ch8",
    },
    Source {
        name: "dmatlack/chip8",
        repository: "dmatlack/chip8",
        revision: "master",
        path_prefix: "roms/",
        catalogue_path: None,
        extension: ".ch8",
    },
    Source {
        name: "Octo examples — John Earnest",
        repository: "JohnEarnest/Octo",
        revision: "gh-pages",
        path_prefix: "examples/",
        catalogue_path: None,
        extension: ".8o",
    },
];

#[derive(Clone, Debug)]
pub struct Game {
    pub name: String,
    pub source: String,
    pub download_url: String,
    pub profile: CompatibilityProfile,
    pub palette: Option<Palette>,
    pub launch_kind: LaunchKind,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LaunchKind {
    BinaryRom,
    OctoSource,
}

pub enum Update {
    Loaded(Vec<Game>),
    Status(String),
    Log(String),
    LoadedGame { game: Game, bytes: Vec<u8> },
    Failed(String),
}

pub struct RomLibrary {
    sender: mpsc::Sender<Update>,
    receiver: mpsc::Receiver<Update>,
    pub games: Vec<Game>,
    pub status: String,
    pub logs: Vec<String>,
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
            logs: Vec::new(),
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
                Update::Status(status) => {
                    self.status = status.clone();
                    self.record_log(status);
                }
                Update::Log(log) => self.record_log(log),
                Update::Failed(error) => {
                    self.status = error.clone();
                    self.record_log(error);
                }
                Update::LoadedGame { .. } => download = Some(update),
            }
        }
        download
    }

    pub fn launch(&mut self, game: Game) {
        self.logs.clear();
        self.status = format!("Téléchargement de {}…", game.name);
        let download_sender = self.sender.clone();
        thread::spawn(move || match load_game(&game, &download_sender) {
            Ok(bytes) => {
                send_status(&download_sender, format!("Lancement de {}…", game.name));
                let _ = download_sender.send(Update::LoadedGame { game, bytes });
            }
            Err(error) => {
                let _ = download_sender.send(Update::Failed(error));
            }
        });
    }

    fn record_log(&mut self, entry: String) {
        const MAX_LOG_ENTRIES: usize = 500;
        if self.logs.len() == MAX_LOG_ENTRIES {
            self.logs.remove(0);
        }
        self.logs.push(entry);
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
        .filter(|entry| is_supported_entry(entry, source))
        .map(|entry| {
            let profile = if source.extension == ".8o" {
                CompatibilityProfile::XoChip
            } else {
                profile_for(&entry.path, &catalogue)
            };
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
                launch_kind: if source.extension == ".8o" {
                    LaunchKind::OctoSource
                } else {
                    LaunchKind::BinaryRom
                },
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

fn is_supported_entry(entry: &TreeEntry, source: Source) -> bool {
    entry.kind == "blob"
        && entry.path.starts_with(source.path_prefix)
        && entry.path.to_ascii_lowercase().ends_with(source.extension)
}

fn load_game(game: &Game, sender: &mpsc::Sender<Update>) -> Result<Vec<u8>, String> {
    send_status(sender, format!("Téléchargement de {}…", game.name));
    let source = download_file(game)?;
    match game.launch_kind {
        LaunchKind::BinaryRom => Ok(source),
        LaunchKind::OctoSource => {
            send_status(sender, "Compilation Docker…".into());
            compile_octo_source(&source, sender)
        }
    }
}

fn send_status(sender: &mpsc::Sender<Update>, status: String) {
    let _ = sender.send(Update::Status(status));
}

fn send_log(sender: &mpsc::Sender<Update>, log: impl Into<String>) {
    let _ = sender.send(Update::Log(log.into()));
}

fn download_file(game: &Game) -> Result<Vec<u8>, String> {
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

const OCTO_IMAGE: &str = "node:22-alpine";
const COMPILER_COMMAND: &str = r#"apk add --no-cache curl; curl --fail --location --retry 3 --output /tmp/compiler.js https://raw.githubusercontent.com/JohnEarnest/Octo/gh-pages/js/compiler.js; node -e 'const fs = require("fs"); const { Compiler } = require("/tmp/compiler.js"); const source = fs.readFileSync(process.argv[1], "utf8"); const compiler = new Compiler(source); compiler.go(); fs.writeFileSync(process.argv[2], Buffer.from(compiler.rom));' /workspace/source.8o /workspace/game.ch8"#;

fn compile_octo_source(source: &[u8], sender: &mpsc::Sender<Update>) -> Result<Vec<u8>, String> {
    let workspace = tempfile::tempdir().map_err(|error| format!("dossier temporaire : {error}"))?;
    fs::write(workspace.path().join("source.8o"), source)
        .map_err(|error| format!("écriture de la source Octo : {error}"))?;
    let runtime = tokio::runtime::Runtime::new()
        .map_err(|error| format!("initialisation du runtime Docker : {error}"))?;
    runtime.block_on(compile_in_docker(workspace.path(), sender))
}

async fn compile_in_docker(
    workspace: &Path,
    sender: &mpsc::Sender<Update>,
) -> Result<Vec<u8>, String> {
    send_log(sender, "Connexion au daemon Docker…");
    let docker = Docker::connect_with_local_defaults().map_err(|error| {
        format!("Docker est indisponible : installez et démarrez Docker ({error})")
    })?;
    docker.ping().await.map_err(|error| {
        format!("Docker est indisponible : installez et démarrez Docker ({error})")
    })?;
    ensure_image(&docker, sender).await?;

    let config = compiler_container_config(workspace);
    let created = docker
        .create_container(
            Some(CreateContainerOptions {
                name: "",
                platform: None,
            }),
            config,
        )
        .await
        .map_err(|error| format!("création du conteneur Docker : {error}"))?;
    let container_id = created.id;
    send_log(
        sender,
        "Conteneur Docker démarré : compilation Octo en cours…",
    );
    let result = run_container(&docker, &container_id, sender).await;
    let cleanup = docker
        .remove_container(
            &container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await;
    if let Err(error) = cleanup {
        return Err(format!("nettoyage du conteneur Docker : {error}"));
    }
    result?;
    let rom = fs::read(workspace.join("game.ch8"))
        .map_err(|error| format!("ROM compilée absente : {error}"))?;
    if rom.is_empty() {
        return Err("ROM compilée vide".into());
    }
    Ok(rom)
}

async fn ensure_image(docker: &Docker, sender: &mpsc::Sender<Update>) -> Result<(), String> {
    if docker.inspect_image(OCTO_IMAGE).await.is_ok() {
        send_log(
            sender,
            format!("Image Docker {OCTO_IMAGE} déjà disponible."),
        );
        return Ok(());
    }
    send_log(
        sender,
        format!("Téléchargement de l’image Docker {OCTO_IMAGE}…"),
    );
    let mut pull = docker.create_image(
        Some(CreateImageOptions {
            from_image: OCTO_IMAGE,
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(progress) = pull.next().await {
        let progress = progress
            .map_err(|error| format!("téléchargement de l'image Docker {OCTO_IMAGE} : {error}"))?;
        if let Some(status) = progress.status {
            send_log(sender, format!("Docker : {status}"));
        }
    }
    Ok(())
}

fn compiler_container_config(workspace: &Path) -> Config<String> {
    Config {
        image: Some(OCTO_IMAGE.into()),
        cmd: Some(vec!["sh".into(), "-ec".into(), COMPILER_COMMAND.into()]),
        host_config: Some(HostConfig {
            mounts: Some(vec![Mount {
                target: Some("/workspace".into()),
                source: Some(workspace.display().to_string()),
                typ: Some(MountTypeEnum::BIND),
                read_only: Some(false),
                ..Default::default()
            }]),
            ..Default::default()
        }),
        ..Default::default()
    }
}

async fn run_container(
    docker: &Docker,
    container_id: &str,
    sender: &mpsc::Sender<Update>,
) -> Result<(), String> {
    docker
        .start_container(container_id, None::<StartContainerOptions<String>>)
        .await
        .map_err(|error| format!("démarrage du conteneur Docker : {error}"))?;
    let log_docker = docker.clone();
    let log_container_id = container_id.to_owned();
    let log_sender = sender.clone();
    let log_task = tokio::spawn(async move {
        let mut logs = log_docker.logs(
            &log_container_id,
            Some(LogsOptions::<String> {
                follow: true,
                stdout: true,
                stderr: true,
                ..Default::default()
            }),
        );
        while let Some(entry) = logs.next().await {
            match entry {
                Ok(entry) => send_log(&log_sender, entry.to_string().trim_end().to_owned()),
                Err(error) => send_log(&log_sender, format!("Lecture des logs Docker : {error}")),
            }
        }
    });
    let result = docker
        .wait_container(container_id, None::<WaitContainerOptions<String>>)
        .next()
        .await
        .ok_or_else(|| "le conteneur Docker n'a retourné aucun résultat".to_owned())
        .and_then(|result| {
            result.map_err(|error| format!("attente du conteneur Docker : {error}"))
        })?;
    log_task
        .await
        .map_err(|error| format!("lecture des logs Docker : {error}"))?;
    if result.status_code == 0 {
        return Ok(());
    }
    Err(format!(
        "compilation Octo échouée (code {})",
        result.status_code
    ))
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
    let filename = path.rsplit('/').next().unwrap_or(path);
    filename
        .strip_suffix(".ch8")
        .or_else(|| filename.strip_suffix(".8o"))
        .unwrap_or(filename)
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
        "schip-1.0" | "schip1.0" => CompatibilityProfile::SuperChip10,
        "schip-1.1" | "schip1.1" => CompatibilityProfile::SuperChip11,
        "schipc" => CompatibilityProfile::SuperChipCompatibility,
        "schip-modern" | "superchip-modern" => CompatibilityProfile::SuperChipModern,
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

    #[test]
    fn discovers_octo_examples_but_not_other_files() {
        let octo = SOURCES[2];
        assert!(is_supported_entry(
            &TreeEntry {
                path: "examples/demos/Murder.8o".into(),
                kind: "blob".into(),
            },
            octo
        ));
        assert!(is_supported_entry(
            &TreeEntry {
                path: "examples/tests/opcode.8o".into(),
                kind: "blob".into(),
            },
            octo
        ));
        assert!(!is_supported_entry(
            &TreeEntry {
                path: "tools/compiler.8o".into(),
                kind: "blob".into(),
            },
            octo
        ));
        assert!(!is_supported_entry(
            &TreeEntry {
                path: "examples/readme.md".into(),
                kind: "blob".into(),
            },
            octo
        ));
    }

    #[test]
    fn octo_examples_use_xo_chip_and_warn_before_launching() {
        let game = Game {
            name: game_name("examples/demos/Murder.8o"),
            source: "Octo examples — John Earnest".into(),
            download_url: "https://example.invalid/Murder.8o".into(),
            profile: CompatibilityProfile::XoChip,
            palette: None,
            launch_kind: LaunchKind::OctoSource,
        };
        assert_eq!(game.name, "Murder");
        assert_eq!(game.profile, CompatibilityProfile::XoChip);
        assert_eq!(game.launch_kind, LaunchKind::OctoSource);
    }

    #[test]
    fn compiler_container_uses_fixed_command_and_workspace_mount() {
        let workspace = tempfile::tempdir().expect("temporary workspace");
        let config = compiler_container_config(workspace.path());
        assert_eq!(config.image.as_deref(), Some(OCTO_IMAGE));
        assert_eq!(
            config.cmd.as_ref().and_then(|command| command.get(2)),
            Some(&COMPILER_COMMAND.to_owned())
        );
        let mount = &config
            .host_config
            .as_ref()
            .expect("host config")
            .mounts
            .as_ref()
            .expect("workspace mount")[0];
        assert_eq!(mount.target.as_deref(), Some("/workspace"));
        assert_eq!(
            mount.source.as_deref(),
            Some(workspace.path().to_str().expect("utf-8 path"))
        );
        assert!(!COMPILER_COMMAND.contains("download_url"));
    }

    #[test]
    fn library_keeps_the_latest_loading_status_until_a_game_is_ready() {
        let (sender, receiver) = mpsc::channel();
        let mut library = RomLibrary {
            sender,
            receiver,
            games: Vec::new(),
            status: String::new(),
            logs: Vec::new(),
            filter: String::new(),
            profile_filter: None,
        };
        let game = Game {
            name: "Example".into(),
            source: "test".into(),
            download_url: "https://example.invalid/example.ch8".into(),
            profile: CompatibilityProfile::OriginalChip8,
            palette: None,
            launch_kind: LaunchKind::BinaryRom,
        };
        library
            .sender
            .send(Update::Status("Téléchargement de Example…".into()))
            .expect("status delivered");
        library
            .sender
            .send(Update::Status("Lancement de Example…".into()))
            .expect("status delivered");
        library
            .sender
            .send(Update::LoadedGame {
                game,
                bytes: vec![0x00, 0xE0],
            })
            .expect("loaded game delivered");

        assert!(matches!(
            library.receive_updates(),
            Some(Update::LoadedGame { .. })
        ));
        assert_eq!(library.status, "Lancement de Example…");
        assert_eq!(library.logs.len(), 2);
    }
}
