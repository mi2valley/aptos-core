// Copyright (c) Aptos
// SPDX-License-Identifier: Apache-2.0

use crate::{
    configuration::{EvaluatorArgs, NodeAddress},
    evaluator::{EvaluationResult, Evaluator},
    evaluators::EvaluatorType,
};
use anyhow::{anyhow, format_err, Result};
use aptos_config::config::RoleType;
use aptos_sdk::types::chain_id::ChainId;
use clap::Parser;
use poem_openapi::Object as PoemObject;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display, str::FromStr};
use thiserror::Error as ThisError;

use super::{super::DirectEvaluatorInput, API_CATEGORY};

/// This function hits the `/` endpoint of the API and returns the chain ID
/// and role type, extracted from the IndexResponse.
pub async fn get_node_identity(node_address: &NodeAddress) -> Result<(ChainId, RoleType)> {
    let mut url = node_address.url.clone();
    url.set_port(Some(node_address.api_port))
        .map_err(|_| format_err!("Failed to set port for URL"))?;

    let client = reqwest::ClientBuilder::new()
        .timeout(std::time::Duration::from_secs(4))
        .build()
        .unwrap();

    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| format_err!("Failed to get node identity {}", e))?;
    let response_body = response
        .text()
        .await
        .map_err(|e| format_err!("Failed to get body of node identity response {}", e))?;

    let data: HashMap<String, serde_json::Value> =
        serde_json::from_str(&response_body).map_err(|e| {
            format_err!(
                "Failed to process response body as valid JSON with string key/values {}",
                e
            )
        })?;

    let chain_id_raw: u8 = data
        .get("chain_id")
        .ok_or_else(|| format_err!("Failed to get chain_id from node identity"))?
        .as_u64()
        .ok_or_else(|| anyhow!("Failed to read chain ID from node identity as u8"))?
        as u8;
    let chain_id = ChainId::new(chain_id_raw);

    let role_type_raw = data
        .get("node_role")
        .ok_or_else(|| format_err!("Failed to get node_role from node identity"))?
        .as_str()
        .ok_or_else(|| anyhow!("Failed to read node_role from node identity as str"))?;
    let role_type = RoleType::from_str(role_type_raw)
        .map_err(|e| format_err!("Failed to parse node_role {}", e))?;

    Ok((chain_id, role_type))
}

#[derive(Debug, ThisError)]
pub enum NodeIdentityEvaluatorError {}

// TODO: Consider taking chain_id and role_type here instead.
#[derive(Clone, Debug, Deserialize, Parser, PoemObject, Serialize)]
pub struct NodeIdentityEvaluatorArgs {}

#[allow(dead_code)]
#[derive(Debug)]
pub struct NodeIdentityEvaluator {
    args: NodeIdentityEvaluatorArgs,
}

impl NodeIdentityEvaluator {
    pub fn new(args: NodeIdentityEvaluatorArgs) -> Self {
        Self { args }
    }

    fn help_build_evaluation_result<T: Display + PartialEq>(
        &self,
        baseline_value: T,
        target_value: T,
        attribute_str: &str,
    ) -> EvaluationResult {
        let (headline, score, explanation) = if baseline_value == target_value {
            (
                format!("{} reported by baseline and target match", attribute_str),
                100,
                format!(
                    "The node under investigation reported the same {} {} \
                as is reported by the baseline node",
                    attribute_str, target_value
                ),
            )
        } else {
            (
                format!(
                    "{} reported by the target does not match the baseline",
                    attribute_str
                ),
                0,
                format!(
                    "The node under investigation reported the {} {}  while the \
                baseline reported {}. These values should match. Confirm that \
                the baseline you're using is appropriate for the node you're testing.",
                    attribute_str, target_value, baseline_value
                ),
            )
        };
        self.build_evaluation_result(headline, score, explanation)
    }
}

#[async_trait::async_trait]
impl Evaluator for NodeIdentityEvaluator {
    type Input = DirectEvaluatorInput;
    type Error = NodeIdentityEvaluatorError;

    /// Assert that the node identity (role type and chain ID) of the two nodes match.
    async fn evaluate(&self, input: &Self::Input) -> Result<Vec<EvaluationResult>, Self::Error> {
        let (target_chain_id, target_role_type) =
            match get_node_identity(&input.target_node_address).await {
                Ok((chain_id, role_type)) => (chain_id, role_type),
                Err(e) => {
                    return Ok(vec![self.build_evaluation_result(
                        "Failed to get node identity from target node".to_string(),
                        0,
                        format!(
                            "Failed to get node identity from target node, \
                        make sure your API port ({}) is open and you're running \
                        the correct node version: {}",
                            input.target_node_address.api_port, e
                        ),
                    )])
                }
            };

        let evaluation_results = vec![
            self.help_build_evaluation_result(
                input.baseline_node_information.chain_id,
                target_chain_id,
                "Chain ID",
            ),
            self.help_build_evaluation_result(
                input.baseline_node_information.role_type,
                target_role_type,
                "Role Type",
            ),
        ];

        Ok(evaluation_results)
    }

    fn get_category_name() -> String {
        API_CATEGORY.to_string()
    }

    fn get_evaluator_name() -> String {
        "node_identity".to_string()
    }

    fn from_evaluator_args(evaluator_args: &EvaluatorArgs) -> Result<Self> {
        Ok(Self::new(evaluator_args.node_identity_args.clone()))
    }

    fn evaluator_type_from_evaluator_args(_: &EvaluatorArgs) -> Result<EvaluatorType> {
        unreachable!();
    }
}
