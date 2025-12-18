use zed::serde_json;
use zed_extension_api as zed;

pub const DEBUG_ADAPTER_NAME: &str = "UnityDAP";
pub const UNITY_DAP_GITHUB: &str = "walcht/unity-dap";
pub const UNITY_DAP_ASSET_NAME: &str = "unity-debug-adapter.zip";
pub const UNITY_DAP_DIR_NAME: &str = "unity-debug-adapter";
pub const UNITY_DAP_BINARY_NAME: &str = "unity-debug-adapter";

#[derive(Default)]
struct UnityEngineExtension {
    cached_dap_binary: Option<UnityDapBinary>,
}

struct UnityDapBinary {
    version: String,
    path: String,
}

impl zed::Extension for UnityEngineExtension {
    fn new() -> Self
    where
        Self: Sized,
    {
        Self::default()
    }

    fn dap_request_kind(
        &mut self,
        _adapter_name: String,
        _config: zed::serde_json::Value,
    ) -> zed::Result<zed::StartDebuggingRequestArgumentsRequest, String> {
        Ok(zed::StartDebuggingRequestArgumentsRequest::Attach)
    }

    fn dap_config_to_scenario(
        &mut self,
        config: zed::DebugConfig,
    ) -> zed::Result<zed::DebugScenario, String> {
        let _request = match config.request {
            zed::DebugRequest::Attach(request) => request,
            _ => return Err("UnityDAP only supports attaching to running processes".into()),
        };

        Ok(zed::DebugScenario {
            label: config.label,
            adapter: config.adapter,
            build: None,
            // TODO: find a valid TCP listen port via request.process_id and pass into config.
            config: "{}".into(),
            tcp_connection: None,
        })
    }

    fn get_dap_binary(
        &mut self,
        adapter_name: String,
        config: zed::DebugTaskDefinition,
        user_provided_debug_adapter_path: Option<String>,
        worktree: &zed::Worktree,
    ) -> zed::Result<zed::DebugAdapterBinary, String> {
        if adapter_name != DEBUG_ADAPTER_NAME {
            return Err(format!(
                "debug adapter must be set to '{}' (requested '{}')",
                DEBUG_ADAPTER_NAME, adapter_name,
            ));
        }

        let (platform, _arch) = zed::current_platform();

        let dap_config = serde_json::from_str::<serde_json::Value>(&config.config)
            .map_err(|err| format!("failed to parse config: {}", err))?;
        let mono_path = dap_config
            .get("monoPath")
            .and_then(|value| value.as_str())
            .unwrap_or("mono")
            .to_string();
        let log_level = dap_config
            .get("logLevel")
            .and_then(|value| value.as_str())
            .unwrap_or(if cfg!(debug_assertions) {
                "trace"
            } else {
                "warn"
            });
        // Connection info for the Unity session to debug:
        let address = dap_config
            .get("address")
            .and_then(|value| value.as_str())
            .map_or_else(
                || Ok(std::net::Ipv4Addr::LOCALHOST),
                |address| {
                    address.parse().map_err(|err| {
                        format!("failed to parse valid address `{}`: {}", address, err)
                    })
                },
            )?
            .to_string();
        let port = dap_config
            .get("port")
            .and_then(|value| value.as_u64())
            .and_then(|port| u16::try_from(port).ok())
            // TODO: try to find TCP port via process name i.e. "Unity.exe"
            .ok_or("must provide a valid port".to_string())?;

        let unity_dap_binary = 'b: {
            if let Some(path) = user_provided_debug_adapter_path {
                break 'b path;
            }

            let release = zed::latest_github_release(
                UNITY_DAP_GITHUB,
                zed::GithubReleaseOptions {
                    require_assets: true,
                    pre_release: false,
                },
            )
            .map_err(|err| {
                format!(
                    "failed to fetch latest github release of unity-debug-adapter: {}",
                    err
                )
            })?;

            if let Some(binary) = &self.cached_dap_binary {
                if &binary.version >= &release.version {
                    break 'b binary.path.clone();
                }
            }

            let version_dir = format!("{}-{}", UNITY_DAP_DIR_NAME, release.version);
            let binary_name = format!("{}.{}",
                UNITY_DAP_BINARY_NAME,
                match platform {
                    zed::Os::Windows => "exe",
                    _ => return Err("automatic download of unity-debug-adapter is currently only supported on windows".into()),
                },
            );

            let cwd = std::env::current_dir()
                .map_err(|err| format!("failed to read working directory: {}", err))?;
            let mut existing_versions = std::fs::read_dir(&cwd)
                .map_err(|err| format!("failed to read working directory: {}", err))?
                .filter_map(|dir| {
                    dir.ok().filter(|dir| {
                        dir.file_name()
                            .to_str()
                            .filter(|name| name.starts_with(UNITY_DAP_DIR_NAME))
                            .is_some()
                    })
                })
                .collect::<Vec<_>>();
            existing_versions.sort_by(|a, b| {
                a.file_name()
                    .to_str()
                    .unwrap()
                    .cmp(b.file_name().to_str().unwrap())
            });
            if let Some(latest_existing) = existing_versions.last() {
                if latest_existing.file_name().to_str().unwrap() >= &version_dir {
                    let mut path = latest_existing.path();
                    path.extend(["Release", &binary_name]);
                    break 'b path.to_string_lossy().to_string();
                }
            }

            let asset = release
                .assets
                .into_iter()
                .find(|asset| asset.name == UNITY_DAP_ASSET_NAME)
                .ok_or("failed to find a valid build of unity-debug-adapter".to_string())?;
            zed::download_file(
                &asset.download_url,
                &version_dir,
                zed::DownloadedFileType::Zip,
            )
            .map_err(|err| format!("failed to download unity-debug-adapter: {}", err))?;

            let mut binary_path = cwd;
            binary_path.extend([&version_dir, "Release", &binary_name]);
            let binary_path = binary_path.to_string_lossy().to_string();
            zed::make_file_executable(&binary_path)
                .map_err(|err| format!("failed to make {} executable: {}", binary_path, err))?;
            self.cached_dap_binary = Some(UnityDapBinary {
                version: release.version,
                path: binary_path.clone(),
            });
            binary_path
        };

        if !std::fs::exists(&unity_dap_binary).unwrap_or(false) {
            return Err(format!(
                "unity-debug-adapter does not exist at expected path: {}",
                unity_dap_binary
            ));
        }

        Ok(zed::DebugAdapterBinary {
            command: Some(mono_path),
            arguments: vec![unity_dap_binary, format!("--log-level={}", log_level)],
            envs: vec![],
            cwd: Some(worktree.root_path()),
            connection: None,
            request_args: zed::StartDebuggingRequestArguments {
                configuration: serde_json::json!({
                    "address": address,
                    "port": port,
                })
                .to_string(),
                request: self.dap_request_kind(adapter_name, dap_config)?,
            },
        })
    }
}

zed::register_extension!(UnityEngineExtension);
