# SLReader

SLReader is a free, open source desktop app for reading journal PDFs, screening them (Include / Exclude), and extracting data for Systematic Literature Reviews (PRISMA).

> **Status: early development.** The app has not been scaffolded yet, so there is nothing to run or download. The features below describe what is planned for the first release.

## Planned features

- Multiple libraries, each a plain folder on your disk (SQLite database plus copies of your PDFs), up to 250 papers per library
- PDF reader with zoom, text search, normal/dark/sepia page modes, and last-read page
- Highlights in four colors with notes and extraction tags
- Include / Exclude / Undecided decisions with exclusion reasons and assessment notes
- Reference list import and `[#n]` links between notes
- Export to CSV and Markdown, plus JSON backup and restore
- Optional AI assistance with a provider you choose (Claude or any OpenAI-compatible endpoint, including local models): chat with a paper, chat across a library, and screening suggestions

The first release targets Windows only. macOS and Linux builds are planned for later.

## Privacy

- No accounts, no server, no telemetry. Your data stays on your computer.
- AI is optional. Paper text is sent only to the AI provider you have set up and approved.
- API keys are stored in the operating system keychain, never in the library or in exports.
- AI never changes a screening decision. Only you do.

## Tech stack

- [Tauri 2](https://tauri.app/) desktop shell
- Rust core: SQLite, files, keychain, text extraction, local embeddings, AI provider calls
- [Svelte](https://svelte.dev/) user interface with [PDF.js](https://mozilla.github.io/pdf.js/) as the reader

## Building from source

### Prerequisites (Windows 10 or 11)

1. [Node.js](https://nodejs.org/) LTS (includes npm)
2. [Rust](https://www.rust-lang.org/tools/install), installed with `rustup` using the MSVC toolchain
3. [Microsoft C++ Build Tools](https://visualstudio.microsoft.com/visual-cpp-build-tools/) with the "Desktop development with C++" workload
4. [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (already included in most Windows 10 and 11 installations)

See the [Tauri prerequisites guide](https://tauri.app/start/prerequisites/) for details.

### Commands

These commands will work once the application scaffold is in the repository.

```bash
npm install            # install UI dependencies
npm run tauri dev      # run the app in development mode
npm run tauri build    # build the Windows installer
cargo test --manifest-path src-tauri/Cargo.toml   # Rust tests
npm run test           # UI tests
```

## Contributing

Contributions are welcome. Please read [CONTRIBUTING.md](CONTRIBUTING.md) first.

## License

[MIT](LICENSE)
