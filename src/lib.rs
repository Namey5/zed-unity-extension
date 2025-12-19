use zed::serde_json;
use zed_extension_api as zed;

zed::register_extension!(UnityEngineExtension);

const DEBUG_ADAPTER_NAME: &str = "UnityDAP";
const UNITY_DAP_GITHUB: &str = "walcht/unity-dap";
const UNITY_DAP_ASSET_NAME: &str = "unity-debug-adapter.zip";
const UNITY_DAP_DIR_NAME: &str = "unity-debug-adapter";
const UNITY_DAP_BINARY_NAME: &str = "unity-debug-adapter.exe";

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
        Self {
            ..Default::default()
        }
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
        let zed::DebugRequest::Attach(request) = config.request else {
            return Err(format!(
                "UnityDAP only supports attaching to running processes"
            ));
        };

        Ok(zed::DebugScenario {
            label: config.label,
            adapter: config.adapter,
            build: None,
            config: request
                .process_id
                .map(|process_id| {
                    // From https://github.com/Unity-Technologies/MonoDevelop.Debugger.Soft.Unity/blob/additional-debugger-info/UnityProcessDiscovery.cs#L80
                    let port = 56000 + process_id % 1000;
                    serde_json::json!({
                        "port": port,
                    })
                })
                .unwrap_or_else(|| serde_json::json!({}))
                .to_string(),
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
            .and_then(|value| value.as_str().map(ToString::to_string))
            .or(user_provided_debug_adapter_path)
            .unwrap_or(
                match platform {
                    zed::Os::Windows => "mono.exe",
                    _ => "mono",
                }
                .to_string(),
            );

        let log_level = dap_config
            .get("logLevel")
            .and_then(|value| value.as_str())
            .unwrap_or(if cfg!(debug_assertions) {
                "debug"
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
            .ok_or_else(|| format!("must provide a valid port"))?;

        let unity_dap_binary = self.get_unity_dap_binary()?;

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

impl UnityEngineExtension {
    fn get_unity_dap_binary(self: &mut Self) -> Result<String, String> {
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

        if let Some(cached) = &self.cached_dap_binary
            && cached.version >= release.version
        {
            return Ok(cached.path.clone());
        }

        let cwd = std::env::current_dir()
            .map_err(|err| format!("failed to open working directory: {}", err))?;
        // Find all existing versions of unity-debug-adapter.
        let mut existing_versions = std::fs::read_dir(&cwd)
            .map_err(|err| format!("failed to read working directory: {}", err))?
            .filter_map(|dir| {
                dir.ok().and_then(|dir| {
                    dir.file_name()
                        .to_str()
                        .filter(|name| name.starts_with(UNITY_DAP_DIR_NAME))
                        .map(|name| name.to_string())
                })
            })
            .collect::<Vec<_>>();
        // These should be named by version, so sort in ascending order.
        existing_versions.sort_by(String::cmp);

        let version_dir = format!("{}-{}", UNITY_DAP_DIR_NAME, release.version);
        // Compare with the latest existing version and use that if it is the same or newer.
        let version_dir = if let Some(latest_existing) = existing_versions.into_iter().last()
            && latest_existing >= version_dir
        {
            latest_existing
        } else {
            let asset = release
                .assets
                .into_iter()
                .find(|asset| asset.name == UNITY_DAP_ASSET_NAME)
                .ok_or_else(|| format!("failed to find a valid build of unity-debug-adapter"))?;
            zed::download_file(
                &asset.download_url,
                &version_dir,
                zed::DownloadedFileType::Zip,
            )
            .map_err(|err| format!("failed to download unity-debug-adapter: {}", err))?;

            version_dir
        };

        // Need the absolute path.
        let mut binary_path = cwd;
        binary_path.extend([&version_dir, "Release", UNITY_DAP_BINARY_NAME]);
        let binary_path = binary_path.to_string_lossy().to_string();
        if !std::fs::exists(&binary_path).unwrap_or(false) {
            return Err(format!(
                "unity-debug-adapter does not exist at expected path: {}",
                binary_path
            ));
        }

        self.cached_dap_binary = Some(UnityDapBinary {
            version: release.version,
            path: binary_path.clone(),
        });

        Ok(binary_path)
    }
}
