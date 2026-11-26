# Pi Config App

Pi Config App is a native desktop editor for [Pi user settings](https://docs.pi.dev/latest/settings). It edits [24 of the 61 settings keys in Pi 1.6.0](docs/prd/pi-config-app.md#可編輯欄位), including model defaults, interaction, terminal appearance, images, Markdown, and changelog display. It uses [Rune](https://github.com/anomalyco/rune) and works offline. It does not edit project settings or access credentials.

## Run on Linux

On Debian or Ubuntu, install the native dependencies, then build and run with a current stable Rust toolchain:

```sh
sudo apt-get install libxkbcommon-dev libvulkan1 mesa-vulkan-drivers
cargo run --locked
```

Hover over the settings path on the right side of the title bar to see **Click to copy the full path**. Click the path to copy it in full, even when the displayed text is shortened. If `PI_CODING_AGENT_DIR` is unset or empty, the app opens `~/.pi/agent/settings.json`. To use another directory:

```sh
PI_CODING_AGENT_DIR="$HOME/my-pi-agent" cargo run --locked
```

Choose a settings group, edit one or more values, then click **Save**. Each group remembers its scroll position and pending edits when you switch groups. Status messages appear to the left of **Reload** and **Save**. If an invalid value blocks saving, click **Go to field** to correct it. The **Appearance** group separates fullscreen preferences from other settings; they can be edited in any mode but only apply in fullscreen. To remove an override, click **Clear** next to a text or number field or choose **Use Pi default** from a menu. For Markdown code block indent, **Set empty** saves an explicit empty string instead of using Pi's default indentation.

A short gray line to the left of a setting marks a value that differs from Pi 1.6.0's built-in default, including an explicit provider or model in place of automatic selection. The line updates as you edit. The comparison does not account for environment variables or project settings.

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

Add `--chrome` to the preview command to show the macOS window buttons and check that they do not overlap the path in the title bar.

See the [product requirements](docs/prd/pi-config-app.md) for scope and acceptance checks. Linux is the first target; macOS and Windows are not verified.
