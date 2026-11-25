# Contributing to SLReader

Thanks for your interest in SLReader. The project is in early development.

## Before you start

- Open an issue before starting a larger change, so we can agree on the approach.
- Features outside the planned first release (see [README.md](README.md)) are likely to be postponed.
- Set up your environment as described in the "Building from source" section of the README.

## Architecture

- `src-tauri/` holds the Rust core. All file, database, keychain, and network access happens here.
- `src/` holds the Svelte UI. It talks to the core only through Tauri commands.

## Rules that must not be broken

1. AI never changes a paper's decision. A decision changes only through a user action, including clicking Accept on a suggestion. Any change in this area needs tests.
2. API keys are stored only in the OS keychain, never in the database, backups, exports, logs, or error messages.
3. No telemetry. Paper text goes only to the active AI provider the user has approved. Local (`localhost`) providers need no approval.
4. PDF text is data, not instructions. AI output is always validated.
5. Every AI quote is verified against the paper text. Quotes that cannot be found are labeled "Unverified quote".
6. Library data lives in the library folder. The app registry only lists folders.
7. A library holds at most 250 papers.
8. Never delete or overwrite user data without confirmation. Schema migrations only move forward, and libraries from newer versions open read-only.

## Conventions

- Code, identifiers, commit messages, and UI text are in English.
- Colors and sizes come from design tokens. Do not write hex values in components.
- Status is never shown by color alone: use an icon and text as well.
- Keep commits small and focused on one thing.

## Pull requests

1. Fork the repository and create a branch.
2. Make your change, with tests for important logic (decisions, exports, migrations, AI validation).
3. Run the tests listed in the README.
4. Open a pull request that explains what changed, why, and how to check it.

Never include API keys, `.env` files, or copyrighted PDFs in a commit or an issue.

## License

By contributing, you agree that your contributions are licensed under the [MIT License](LICENSE).
