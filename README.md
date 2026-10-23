# rotuner

Configure Roblox and Studio with TOML.

```toml
[client]
MasterVolume = 0.5
CameraMode = "Classic"

[studio.Studio]
"Auto Closing Brackets" = true
```

Close Roblox and Studio, then run:

```sh
rotuner apply
```

It checks `./rotuner.toml`, `./config.toml`, the OS config directory's `rotuner/config.toml`, then `~/.config/rotuner/config.toml`. The first file wins. On Windows, the OS config directory is `%APPDATA%`. Use `rotuner apply path/to/config.toml` to choose a file.

Browse settings with `rotuner catalog`. Get an editor schema with `rotuner schema`.

Raw writes accept JSON text for client storage and typed registry values for Studio:

```toml
[raw.storage]
MinimizeToTray = '"false"'

[[raw.registry]]
path = 'Themes'
name = 'CurrentTheme'
data = { type = "REG_SZ", value = "Dark" }
```

Registry paths are relative to `HKCU\Software\Roblox\RobloxStudio`. Raw writes bypass Roblox setting validation.
