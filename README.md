# Pi Config App

Pi Config App is a native desktop editor for three [Pi user settings](https://docs.pi.dev/latest/settings): theme, default thinking level, and whether thinking appears in the transcript. It uses [Rune](https://github.com/anomalyco/rune) and works offline. It does not edit project settings or access credentials.

## Run on Linux

On Debian or Ubuntu, install the native dependencies, then build and run with a current stable Rust toolchain:

```sh
sudo apt-get install libxkbcommon-dev libvulkan1 mesa-vulkan-drivers
cargo run --locked
```

The window shows the path it uses. If `PI_CODING_AGENT_DIR` is unset or empty, the app opens `~/.pi/agent/settings.json`. To use another directory:

```sh
PI_CODING_AGENT_DIR="$HOME/my-pi-agent" cargo run --locked
```

Edit a value and click **Save**. To remove an override, click **Clear** next to the theme or choose **Use Pi default** from a menu. The first save creates a missing settings file. Unsupported values and unedited settings are kept. The app does not open a `settings.json` symbolic link or overwrite a file it detects has changed since load. Use **Reload** to read it again. After saving, use `/reload` in an open Pi session; project settings can still override user preferences.

## Verify changes

From the repository root:

```sh
cargo fmt --all -- --check
cargo clippy --locked --all-targets -- -D warnings
cargo test --locked --all-targets
prek run --all-files
```

Integration tests run the built app in Rune's headless mode with temporary settings directories. For a manual preview without opening a window or touching your Pi settings:

```sh
scratch="$(mktemp -d)"
PI_CODING_AGENT_DIR="$scratch" cargo run --locked -- --snap "$scratch/settings.png" --tree "$scratch/tree.json"
```

See the [product requirements](docs/prd/pi-config-app.md) for scope and acceptance checks. Linux is the first target; macOS and Windows are not verified.
