# CHIP-8mulator

Émulateur CHIP-8, Super-CHIP et XO-CHIP écrit en Rust. Le dépôt contient un
moteur déterministe indépendant de la plateforme, une interface native et une
interface web React/WebAssembly.

## Démo et téléchargements

- [Essayer la démo web](https://clementreiffers.github.io/chip8mulator/)
- [Toutes les releases](https://github.com/clementreiffers/chip8mulator/releases/latest)

| Distribution | Téléchargement |
| --- | --- |
| Interface native — Linux x86_64 | [`.tar.gz`](https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8-native-gui-linux-x86_64.tar.gz) |
| Interface native — Windows x86_64 | [`.zip`](https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8-native-gui-windows-x86_64.zip) |
| Interface native — macOS Intel | [`.tar.gz`](https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8-native-gui-macos-intel.tar.gz) |
| Interface native — macOS Apple Silicon | [`.tar.gz`](https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8-native-gui-macos-arm64.tar.gz) |
| Moteur Rust réutilisable | [archive Cargo `.crate`](https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8-engine.crate) |
| Bindings WebAssembly | [archive WASM](https://github.com/clementreiffers/chip8mulator/releases/latest/download/chip8-engine-wasm.zip) |

Les liens de plateformes pointent vers les assets stables de la dernière
release. Les binaires macOS ne sont pas signés ni notarisés.

## Démarrer le projet

Le moteur et l’interface native sont deux crates Rust distincts. Le front web
est une application React/Vite.

```sh
cargo test --manifest-path src/chip8-engine/Cargo.toml
task play-library
task web-dev
task build-web
```

Consultez les README des sous-projets pour les détails :
[moteur](src/chip8-engine/README.md) et [site web](src/chip8-web/README.md).

## Crédits et démarche

Ce projet a été réalisé avec l’aide d’outils d’IA pour la conception,
l’implémentation, les tests et la documentation. L’IA n’est pas présentée
comme l’unique source de vérité : le comportement de l’émulateur a été vérifié
à l’aide de ROMs de conformité et de la documentation de l’écosystème CHIP-8.

Les ressources suivantes ont particulièrement guidé le travail :

- [Building a CHIP-8 Emulator in C++](https://austinmorlan.com/posts/chip8_emulator/) par Austin Morlan ;
- [CHIP-8 database and opcode reference](https://chip8.gulrak.net/) par Gulrak ;
- l’aide et les échanges avec [im-rises](https://github.com/im-rises).

## Releases

Une release est créée par un tag Git `vX.Y.Z`. La CI vérifie que cette version
est identique dans les manifests Rust et web, construit les exécutables pour
Linux, Windows et macOS, publie le crate et les bindings WASM, puis déploie le
site sur GitHub Pages.
