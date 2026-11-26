# Pi Config App

Pi Config App is a native desktop editor for [Pi user settings](https://docs.pi.dev/latest/settings). It edits [24 of the 61 settings keys in Pi 1.6.0](docs/prd/pi-config-app.md#可編輯欄位), including model defaults, interaction, terminal appearance, images, Markdown, and changelog display. It uses [Rune](https://github.com/anomalyco/rune) and works offline. It does not edit project settings or access credentials.

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

Choose a settings group, edit one or more values, then click **Save**. You can switch groups without losing your edits. To remove an override, click **Clear** next to a text or number field or choose **Use Pi default** from a menu. For Markdown code block indent, **Set empty** saves an explicit empty string instead of using Pi's default indentation.

The first save creates a missing settings file. The app preserves unsupported values and unedited settings, including sibling values in nested objects. It does not open a `settings.json` symbolic link or overwrite a file it detects has changed since load. Use **Reload** to read it again.

After saving, use `/reload` in an open Pi session. Startup settings may require a new session or a Pi restart. Project settings can still override user preferences. The editor does not check whether a provider or model ID is available.

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
