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

Export with `rotuner export > config.toml`. It exports all sources; pass `settings`, `client-storage`, or `studio-registry` to select one.

Raw client JSON and Studio registry values:

```toml
[raw.storage]
MinimizeToTray = '"false"'

[[raw.registry]]
path = 'Themes'
name = 'CurrentTheme'
data = { type = "REG_SZ", value = "Dark" }
```

Registry paths are relative to `HKCU\Software\Roblox\RobloxStudio`. Raw writes bypass Roblox setting validation.
