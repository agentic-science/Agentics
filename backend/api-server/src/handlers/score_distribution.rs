//! Score-distribution builders for public challenge metric views.

use shared::error::{AppError, Result};
use shared::models::names::{ChallengeName, MetricName, TargetName};
use shared::models::request::{
    LeaderboardEntryDto, ScoreDistributionBucketDto, ScoreDistributionQuantileDto,
    ScoreDistributionResponse,
};

/// Build a distribution response from the visible best leaderboard entries in scope.
pub(super) fn build_score_distribution_response(
    challenge_name: ChallengeName,
    target: TargetName,
    metric_name: MetricName,
    entries: Vec<LeaderboardEntryDto>,
) -> Result<ScoreDistributionResponse> {
    let mut values = entries
        .iter()
        .filter_map(|entry| metric_value_from_leaderboard_entry(entry, &metric_name))
        .filter(|value| value.is_finite())
        .collect::<Vec<_>>();
    values.sort_by(f64::total_cmp);
    let count = i64::try_from(values.len())
        .map_err(|_| AppError::Internal("score distribution count overflow".to_string()))?;
    let (min, max, mean, quantiles, histogram) = if values.is_empty() {
        (None, None, None, Vec::new(), Vec::new())
    } else {
        let min = values.first().copied().ok_or_else(|| {
            AppError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let max = values.last().copied().ok_or_else(|| {
            AppError::Internal("score distribution unexpectedly empty".to_string())
        })?;
        let sum: f64 = values.iter().sum();
        let mean = sum / values.len() as f64;
        (
            Some(min),
            Some(max),
            Some(mean),
            build_quantiles(&values)?,
            build_histogram(&values)?,
        )
    };

    Ok(ScoreDistributionResponse {
        challenge_name,
        target,
        metric_name,
        count,
        min,
        max,
        mean,
        quantiles,
        histogram,
    })
}

/// Select the metric value that participates in one distribution.
fn metric_value_from_leaderboard_entry(
    entry: &LeaderboardEntryDto,
    metric_name: &MetricName,
) -> Option<f64> {
    match metric_name.as_str() {
        "rank_score" | "best_rank_score" => Some(entry.best_rank_score),
        "official_score" => entry.official_score,
        _ => entry
            .aggregate_metrics
            .iter()
            .chain(entry.official_metrics.iter())
            .find(|metric| &metric.metric_name == metric_name)
            .map(|metric| metric.value),
    }
}

/// Build nearest-rank quantiles used by the public distribution API.
fn build_quantiles(values: &[f64]) -> Result<Vec<ScoreDistributionQuantileDto>> {
    [
        (0.0, 0usize, 4usize),
        (0.25, 1usize, 4usize),
        (0.5, 2usize, 4usize),
        (0.75, 3usize, 4usize),
        (1.0, 4usize, 4usize),
    ]
    .into_iter()
    .map(|(quantile, numerator, denominator)| {
        Ok(ScoreDistributionQuantileDto {
            quantile,
            value: nearest_rank_quantile(values, numerator, denominator)?,
        })
    })
    .collect()
}

/// Select one nearest-rank quantile from already-sorted finite values.
fn nearest_rank_quantile(values: &[f64], numerator: usize, denominator: usize) -> Result<f64> {
    let max_index = values.len().saturating_sub(1);
    let rounded_index = max_index
        .checked_mul(numerator)
        .and_then(|value| value.checked_add(denominator / 2))
        .and_then(|value| value.checked_div(denominator))
        .ok_or_else(|| AppError::Internal("quantile index overflow".to_string()))?
        .min(max_index);
    values
        .get(rounded_index)
        .copied()
        .ok_or_else(|| AppError::Internal("quantile index out of range".to_string()))
}

/// Build at most ten histogram buckets for already-sorted finite values.
fn build_histogram(values: &[f64]) -> Result<Vec<ScoreDistributionBucketDto>> {
    let min = values
        .first()
        .copied()
        .ok_or_else(|| AppError::Internal("histogram values unexpectedly empty".to_string()))?;
    let max = values
        .last()
        .copied()
        .ok_or_else(|| AppError::Internal("histogram values unexpectedly empty".to_string()))?;
    if min == max {
        return Ok(vec![ScoreDistributionBucketDto {
            lower: min,
            upper: max,
            count: i64::try_from(values.len())
                .map_err(|_| AppError::Internal("histogram count overflow".to_string()))?,
        }]);
    }

    let bucket_count = values.len().min(10);
    let width = (max - min) / bucket_count as f64;
    let mut counts = vec![0i64; bucket_count];
    for value in values {
        let index = histogram_bucket_index(*value, min, width, bucket_count)?;
        let count = counts
            .get_mut(index)
            .ok_or_else(|| AppError::Internal("histogram bucket index invalid".to_string()))?;
        *count = count
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("histogram count overflow".to_string()))?;
    }

    let mut buckets = Vec::with_capacity(counts.len());
    for (index, count) in counts.into_iter().enumerate() {
        let lower = min + width * index as f64;
        let upper = match index.checked_add(1) {
            Some(next_index) if next_index == bucket_count => max,
            Some(next_index) => min + width * next_index as f64,
            None => {
                return Err(AppError::Internal(
                    "histogram bucket index overflow".to_string(),
                ));
            }
        };
        buckets.push(ScoreDistributionBucketDto {
            lower,
            upper,
            count,
        });
    }
    Ok(buckets)
}

/// Locate the histogram bucket for a value without using unchecked indexing.
fn histogram_bucket_index(value: f64, min: f64, width: f64, bucket_count: usize) -> Result<usize> {
    for index in 0..bucket_count {
        let next_index = index
            .checked_add(1)
            .ok_or_else(|| AppError::Internal("histogram bucket index overflow".to_string()))?;
        if next_index == bucket_count {
            return Ok(index);
        }
        let upper = min + width * next_index as f64;
        if value < upper {
            return Ok(index);
        }
    }
    bucket_count
        .checked_sub(1)
        .ok_or_else(|| AppError::Internal("histogram bucket count invalid".to_string()))
}
