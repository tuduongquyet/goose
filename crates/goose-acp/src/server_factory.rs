use anyhow::Result;
use std::sync::Arc;
use tracing::info;

use crate::server::{AcpProviderFactory, GooseAcpAgent};

pub struct AcpServerFactoryConfig {
    pub builtins: Vec<String>,
    pub data_dir: std::path::PathBuf,
    pub config_dir: std::path::PathBuf,
}

pub struct AcpServer {
    config: AcpServerFactoryConfig,
}

impl AcpServer {
    pub fn new(config: AcpServerFactoryConfig) -> Self {
        Self { config }
    }

    pub async fn create_agent(&self) -> Result<Arc<GooseAcpAgent>> {
        let config_path = self
            .config
            .config_dir
            .join(goose::config::base::CONFIG_YAML_NAME);
        let config = goose::config::Config::new(&config_path, "goose")?;

        let goose_mode = config
            .get_goose_mode()
            .unwrap_or(goose::config::GooseMode::Auto);
        let disable_session_naming = config.get_goose_disable_session_naming().unwrap_or(false);

        let provider_factory: AcpProviderFactory =
            Arc::new(move |provider_name, model_config, extensions| {
                Box::pin(async move {
                    goose::providers::create(&provider_name, model_config, extensions).await
                })
            });

        let agent = GooseAcpAgent::new(
            provider_factory,
            self.config.builtins.clone(),
            self.config.data_dir.clone(),
            self.config.config_dir.clone(),
            goose_mode,
            disable_session_naming,
        )
        .await?;
        info!("Created new ACP agent");

        let agent = Arc::new(agent);
        spawn_provider_prewarm(Arc::clone(&agent));
        Ok(agent)
    }
}

/// Best-effort background warm-up of the most-recently-used provider so the
/// user doesn't pay the cold-start cost on the first agent click after launch.
fn spawn_provider_prewarm(agent: Arc<GooseAcpAgent>) {
    tokio::spawn(async move {
        let Some(provider_name) = agent.last_used_provider().await else {
            return;
        };

        let providers = goose::providers::providers().await;
        let Some((metadata, _)) = providers
            .into_iter()
            .find(|(m, _)| m.name == provider_name)
        else {
            return;
        };

        let Ok(model_config) = goose::model::ModelConfig::new(&metadata.default_model) else {
            return;
        };
        let model_config = model_config.with_canonical_limits(&provider_name);

        let _ = agent
            .get_or_create_provider(&provider_name, model_config, Vec::new())
            .await;
    });
}
