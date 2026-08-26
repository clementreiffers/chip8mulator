# chip8-web

React/Vite host for `chip8-engine` compiled to WebAssembly.

## Development

Install `wasm-bindgen-cli` at the version locked by `chip8-engine` and the npm
dependencies, then start Vite:

```sh
cargo install wasm-bindgen-cli --version 0.2.127 --locked
cd src/chip8-web
npm install
npm run dev
```

`npm run build` generates the browser-oriented WASM package, type-checks the
application, and emits the static site in `dist/`. The ROM catalogue is loaded
dynamically from the two binary ROM repositories used by the native host; Octo
source files are intentionally excluded.
