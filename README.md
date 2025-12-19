# Zed Unity Extension
Adds debugging support for the [Unity game engine](https://unity.com/) to [Zed](https://zed.dev/) via [unity-dap](https://github.com/walcht/unity-dap).

> [!CAUTION]
> This extension can connect to a running Unity instance, but I have not been able to get debugging working yet (on Windows 11).
> This may be due to upstream issues in `unity-dap`, but feel free to try and see if it works for you.

## Requirements
`unity-dap` requires an explicit installation of [Mono](https://www.mono-project.com/download/stable/).
> [!WARNING]
> The version of Mono that ships with Unity [will not work](https://github.com/walcht/neovim-unity#unity-debugger-support).

## Configuration
To connect to a running Unity instance on the same machine as the debugger you can use Zed's `Attach` dialog,
set the debugger to `UnityDAP` (bottom right) and select the desired process:

<img width="644" height="591" alt="image" src="https://github.com/user-attachments/assets/7a168a3d-9810-4f4f-8886-abdcb05ba6c4" />

> [!WARNING]
> You need to explicitly tell the extension where to find the Mono runtime binary (i.e. `mono.exe`),
> however there is currently no way to provide this directly due to limitations in Zed's extension API.
> As a workaround, you can assign the absolute path to the `MONO_PATH` environment variable.

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
