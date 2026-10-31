# rotuner

Configure Roblox and Studio with TOML.

```toml
[client]
MasterVolume = 0.5
CameraMode = "Classic"

[studio.Studio]
"Auto Closing Brackets" = true
```

Save as `config.toml`, close Roblox and Studio, then run:

```sh
rotuner apply
```

Use `rotuner apply path/to/config.toml` to choose another file.

Browse settings with `rotuner catalog`; generate an editor schema with `rotuner schema`.

Export all sources with `rotuner export > config.toml`. Pass `settings`, `client-storage`, or `studio-preferences` to select one source.

Raw client JSON and Studio preferences:

```toml
[raw.storage]
MinimizeToTray = '"false"'

[raw.preferences]
"Themes.CurrentTheme" = { type = "string", value = "Default" }
```

Preference keys use dots between groups. Escape literal dots and backslashes with a backslash. Raw writes bypass Roblox setting validation.
