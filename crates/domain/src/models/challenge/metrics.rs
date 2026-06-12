use serde::{Deserialize, Serialize};

use super::super::names::MetricName;
use super::serde_helpers::{
    required_nullable, required_nullable_non_empty_vec, required_nullable_non_empty_vec_schema,
    required_nullable_schema, serialize_empty_vec_as_null,
};

/// Whether a metric is better when it is larger or smaller.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricDirection {
    Maximize,
    Minimize,
}

/// Visibility level for a metric emitted by the evaluator.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum MetricVisibility {
    /// Visible in validation feedback and official result views.
    Public,
    /// Visible only after a ranking-visible official evaluation.
    Official,
}

/// One metric that an evaluator may emit in aggregate or per-run result payloads.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricDefinitionSpec {
    pub name: MetricName,
    pub label: String,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    pub unit: Option<String>,
    pub direction: MetricDirection,
    pub visibility: MetricVisibility,
    #[serde(deserialize_with = "required_nullable")]
    #[schemars(required, schema_with = "required_nullable_schema::<String>")]
    pub metric_description: Option<String>,
}

/// Ranking configuration for a challenge.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RankingSpec {
    pub primary_metric_name: MetricName,
    #[serde(
        deserialize_with = "required_nullable_non_empty_vec",
        serialize_with = "serialize_empty_vec_as_null"
    )]
    #[schemars(
        required,
        schema_with = "required_nullable_non_empty_vec_schema::<MetricName>"
    )]
    pub tie_breaker_metric_names: Vec<MetricName>,
}

/// Metric schema embedded in `spec.json`.
#[derive(Debug, Clone, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct MetricSchemaSpec {
    pub metrics: Vec<MetricDefinitionSpec>,
    pub ranking: RankingSpec,
}

impl MetricSchemaSpec {
    /// Look up a metric definition by name.
    pub fn metric(&self, metric_name: &MetricName) -> Option<&MetricDefinitionSpec> {
        self.metrics
            .iter()
            .find(|metric| &metric.name == metric_name)
    }

    /// Primary ranking metric declared by this challenge.
    pub fn primary_metric(&self) -> Option<&MetricDefinitionSpec> {
        self.metric(&self.ranking.primary_metric_name)
    }
}

impl Default for MetricSchemaSpec {
    /// Handles default for this module.
    fn default() -> Self {
        Self {
            metrics: vec![MetricDefinitionSpec {
                name: MetricName::score(),
                label: "Score".to_string(),
                unit: None,
                direction: MetricDirection::Maximize,
                visibility: MetricVisibility::Public,
                metric_description: Some("Challenge-defined compatibility score.".to_string()),
            }],
            ranking: RankingSpec {
                primary_metric_name: MetricName::score(),
                tie_breaker_metric_names: vec![],
            },
        }
    }
}
