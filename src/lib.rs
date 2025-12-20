use zed::serde_json;
use zed_extension_api as zed;

zed::register_extension!(UnityEngineExtension);

const UNITY_DAP_NAME: &str = "UnityDAP";
const UNITY_DAP_GITHUB: &str = "walcht/unity-dap";
const UNITY_DAP_ASSET_NAME: &str = "unity-debug-adapter.zip";
const UNITY_DAP_DIR_NAME: &str = "unity-debug-adapter";
const UNITY_DAP_BINARY_NAME: &str = "unity-debug-adapter.exe";

#[derive(Default)]
struct UnityEngineExtension {
    cached_dap_binary: Option<String>,
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
        if adapter_name != UNITY_DAP_NAME {
            return Err(format!(
                "debug adapter must be set to `{}` (requested `{}`)",
                UNITY_DAP_NAME, adapter_name,
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
            })
            .to_string();

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

        let request = self.dap_request_kind(adapter_name, dap_config)?;
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
                request,
            },
        })
    }
}

// Ordering of these is important as we prefer using an existing
// local copy over fetching from Github when the versions are equal.
#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum DapBinarySource {
    Github { url: String },
    Local,
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct UnityDapBinary {
    dir_name: String,
    source: DapBinarySource,
}

impl UnityEngineExtension {
    fn get_unity_dap_binary(self: &mut Self) -> Result<String, String> {
        // Only really need to check this once per session - if you want to force an update, you can reload the extension.
        if let Some(cached_path) = &self.cached_dap_binary {
            return Ok(cached_path.clone());
        }

        let cwd = std::env::current_dir()
            .map_err(|err| format!("failed to open working directory: {}", err))?;

        let mut available_versions = vec![];
        let mut errors = vec![];
        let release = zed::latest_github_release(
            UNITY_DAP_GITHUB,
            zed::GithubReleaseOptions {
                require_assets: true,
                pre_release: false,
            },
        )
        .map_err(|err| {
            format!(
                "failed to fetch latest release of `{}`: {}",
                UNITY_DAP_GITHUB, err
            )
        })
        .and_then(|zed::GithubRelease { version, assets }| {
            assets
                .into_iter()
                .find(|asset| asset.name == UNITY_DAP_ASSET_NAME)
                .ok_or_else(|| {
                    format!(
                        "failed to find a suitable asset for latest release `{}` of `{}`",
                        version, UNITY_DAP_GITHUB
                    )
                })
                .map(|asset| (version, asset.download_url))
        });

        match release {
            Ok((version, url)) => available_versions.push(UnityDapBinary {
                dir_name: format!("{}-{}", UNITY_DAP_DIR_NAME, version),
                source: DapBinarySource::Github { url },
            }),
            // Just log this error and fallback to an existing local version.
            Err(err) => errors.push(err),
        }

        // Find all existing versions of unity-debug-adapter.
        available_versions.extend(
            std::fs::read_dir(&cwd)
                .map_err(|err| format!("failed to read working directory: {}", err))?
                .filter_map(|dir| {
                    dir.ok().and_then(|dir| {
                        dir.file_name()
                            .to_str()
                            .filter(|name| name.starts_with(UNITY_DAP_DIR_NAME))
                            .map(|name| UnityDapBinary {
                                dir_name: name.to_string(),
                                source: DapBinarySource::Local,
                            })
                    })
                }),
        );
        // These should be named by version, so sort in descending order.
        available_versions.sort_unstable_by(|a, b| b.cmp(a));

        // Find the latest available version.
        let binary_path = available_versions
            .into_iter()
            .find_map(|dap_binary| {
                if let DapBinarySource::Github { url } = &dap_binary.source {
                    // Download the release now that we know it is the newest.
                    zed::download_file(url, &dap_binary.dir_name, zed::DownloadedFileType::Zip)
                        .inspect_err(|err| {
                            errors.push(format!(
                                "failed to download latest release of unity-debug-adapter from `{}`: {}",
                                url, err
                            ))
                        })
                        .ok()?;
                }

                let binary_path = std::path::PathBuf::from_iter([
                    &dap_binary.dir_name,
                    "Release",
                    UNITY_DAP_BINARY_NAME,
                ])
                .to_string_lossy()
                .to_string();

                if let Ok(true) = std::fs::exists(&binary_path) {
                    Some(binary_path)
                } else {
                    errors.push(format!(
                        "cannot find unity-debug-adapter binary at expected path: {}",
                        binary_path
                    ));
                    None
                }
            })
            // Couldn't find any usable installs, so combine and return all errors.
            .ok_or_else(move || {
                format!(
                    "failed to find a suitable install of `{}`: \n  {}",
                    UNITY_DAP_GITHUB, errors.join("; \n  ")
                )
            })?;

        // Command needs the absolute path.
        let absolute_binary_path = cwd.join(binary_path).to_string_lossy().to_string();
        self.cached_dap_binary = Some(absolute_binary_path.clone());
        Ok(absolute_binary_path)
    }
}
