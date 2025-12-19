# Zed Unity Extension
Adds debugging support for the [Unity game engine](https://unity.com/) to [Zed](https://zed.dev/) via [unity-dap](https://github.com/walcht/unity-dap).

> [!CAUTION]
> This extension can connect to a running Unity instance, but I have not been able to get debugging working yet (on Windows 11).
> This may be due to upstream issues in `unity-dap`, but feel free to try and see if it works for you.

## Requirements
`unity-dap` requires an explicit installation of [Mono](https://www.mono-project.com/download/stable/).
> [!WARNING]
> The version of Mono that ships with Unity [will not work](https://github.com/walcht/neovim-unity#unity-debugger-support).

## Installation
Currently only supported as a dev extension; see [Developing an Extension Locally](https://zed.dev/docs/extensions/developing-extensions#developing-an-extension-locally).

## Configuration
You need to explicitly tell the extension where to find the Mono runtime binary (i.e. `mono.exe`) -
this can be done in a debug task or by overriding the DAP binary in `settings.json`:
```json
{
  "dap": {
    "UnityDAP": {
      "binary": "C:\\Program Files\\Mono\\bin\\mono.exe"
    }
  }
}
```

> [!NOTE]
> There is currently no way to override the `unity-dap` binary from within Zed,
> however it will automatically look for the latest version present in the extension's working directory
> (i.e. `%LOCALAPPDATA%\Zed\extensions\work\unity-engine` on Windows) - if you want to use a custom version of `unity-dap`
> then all you need to do is drop the build into this folder and name it with a higher precedence than the other installs, eg:
> ```
> %LOCALAPPDATA%\Zed\extensions\work\unity-engine\
> | unity-debug-adapter-v0.0.1\    # Automatically fetched Github release.
> | | Release\...
> | unity-debug-adapter-v1.0.0\    # Custom build - higher version will be prioritised.
> | | Release\unity-debug-adapter.exe
> ```

To connect to a running Unity instance on the same machine as the debugger you can use Zed's `Attach` dialog,
set the debugger to `UnityDAP` (bottom right) and select the desired process:

<img width="644" height="591" alt="image" src="https://github.com/user-attachments/assets/7a168a3d-9810-4f4f-8886-abdcb05ba6c4" />

Otherwise, you can add a new debug task and explicitly supply the address and port to connect to:

```json
{
  "label": "Unity Editor",
  "adapter": "UnityDAP",
  "monoPath": "C:\\Program Files\\Mono\\bin\\mono.exe",
  "logLevel": "warn",
  "address": "127.0.0.1",
  "port": 56492
}
```
