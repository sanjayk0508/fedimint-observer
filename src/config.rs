use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use fedimint_core::api::InviteCode;
use fedimint_core::config::{
    ClientConfig, ClientModuleConfig, FederationId, JsonClientConfig, JsonWithKind,
};
use fedimint_core::core::{ModuleInstanceId, ModuleKind};
use fedimint_core::encoding::DynRawFallback;
use fedimint_core::module::registry::ModuleDecoderRegistry;
use fedimint_core::module::CommonModuleInit;
use fedimint_ln_common::bitcoin::hashes::hex::ToHex;
use fedimint_ln_common::LightningCommonInit;
use fedimint_mint_common::MintCommonInit;
use fedimint_wallet_common::WalletCommonInit;

use crate::error::Result;

pub async fn fetch_federation_config(
    Path(invite): Path<InviteCode>,
    State(cache): State<FederationConfigCache>,
) -> Result<Json<JsonClientConfig>> {
    Ok(cache.fetch_config_cached(&invite).await?.into())
}

#[derive(Default, Debug, Clone)]
pub struct FederationConfigCache {
    federations: Arc<tokio::sync::RwLock<HashMap<FederationId, JsonClientConfig>>>,
}

impl FederationConfigCache {
    pub async fn fetch_config_cached(
        &self,
        invite: &InviteCode,
    ) -> anyhow::Result<JsonClientConfig> {
        let federation_id = invite.federation_id();

        if let Some(config) = self.federations.read().await.get(&federation_id).cloned() {
            return Ok(config);
        }

        let config = fetch_config_inner(&invite).await?;
        let mut cache = self.federations.write().await;
        if let Some(replaced) = cache.insert(federation_id, config.clone()) {
            if replaced != config {
                // TODO: use tracing
                eprintln!("Warning, config for federation {federation_id} changed");
            }
        }

        Ok(config)
    }
}

async fn fetch_config_inner(invite: &InviteCode) -> anyhow::Result<JsonClientConfig> {
    let raw_config = ClientConfig::download_from_invite_code(&invite).await?;
    let decoders = get_decoders(raw_config.modules.iter().map(
        |(module_instance_id, module_config)| (*module_instance_id, module_config.kind.clone()),
    ));
    let config = raw_config.redecode_raw(&decoders)?;

    Ok(JsonClientConfig {
        global: config.global,
        modules: config
            .modules
            .into_iter()
            .map(
                |(
                    instance_id,
                    ClientModuleConfig {
                        kind,
                        config: module_config,
                        ..
                    },
                )| {
                    (
                        instance_id,
                        JsonWithKind::new(
                            kind.clone(),
                            match module_config {
                                DynRawFallback::Raw { raw, .. } => raw.to_hex().into(),
                                DynRawFallback::Decoded(decoded) => decoded.to_json().into(),
                            },
                        ),
                    )
                },
            )
            .collect(),
    })
}

fn get_decoders(
    modules: impl IntoIterator<Item = (ModuleInstanceId, ModuleKind)>,
) -> ModuleDecoderRegistry {
    ModuleDecoderRegistry::new(modules.into_iter().filter_map(
        |(module_instance_id, module_kind)| {
            let decoder = match module_kind.as_str() {
                "ln" => LightningCommonInit::decoder(),
                "wallet" => WalletCommonInit::decoder(),
                "mint" => MintCommonInit::decoder(),
                _ => {
                    return None;
                }
            };

            Some((module_instance_id, module_kind, decoder))
        },
    ))
    .with_fallback()
}