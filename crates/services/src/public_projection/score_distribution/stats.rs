//! Distribution summary helpers for score-distribution responses.

use agentics_domain::models::request::{ScoreDistributionBucketDto, ScoreDistributionQuantileDto};
use agentics_error::{Result, ServiceError};

/// Build nearest-rank quantiles used by the public distribution API.
pub(super) fn build_quantiles(values: &[f64]) -> Result<Vec<ScoreDistributionQuantileDto>> {
    [
        (0.0, 0usize, 4usize),
        (0.25, 1usize, 4usize),
        (0.5, 2usize, 4usize),
        (0.75, 3usize, 4usize),
        (0.9, 9usize, 10usize),
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

/// Build at most ten histogram buckets for already-sorted finite values.
pub(super) fn build_histogram(values: &[f64]) -> Result<Vec<ScoreDistributionBucketDto>> {
    let min = values
        .first()
        .copied()
        .ok_or_else(|| ServiceError::Internal("histogram values unexpectedly empty".to_string()))?;
    let max = values
        .last()
        .copied()
        .ok_or_else(|| ServiceError::Internal("histogram values unexpectedly empty".to_string()))?;
    if min == max {
        return Ok(vec![ScoreDistributionBucketDto {
            lower: min,
            upper: max,
            count: i64::try_from(values.len())
                .map_err(|_| ServiceError::Internal("histogram count overflow".to_string()))?,
        }]);
    }

    let bucket_count = values.len().min(10);
    let width = (max - min) / bucket_count as f64;
    let mut counts = vec![0i64; bucket_count];
    for value in values {
        let index = histogram_bucket_index(*value, min, width, bucket_count)?;
        let count = counts
            .get_mut(index)
            .ok_or_else(|| ServiceError::Internal("histogram bucket index invalid".to_string()))?;
        *count = count
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("histogram count overflow".to_string()))?;
    }

    let mut buckets = Vec::with_capacity(counts.len());
    for (index, count) in counts.into_iter().enumerate() {
        let lower = min + width * index as f64;
        let upper = match index.checked_add(1) {
            Some(next_index) if next_index == bucket_count => max,
            Some(next_index) => min + width * next_index as f64,
            None => {
                return Err(ServiceError::Internal(
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

/// Select one nearest-rank quantile from already-sorted finite values.
fn nearest_rank_quantile(values: &[f64], numerator: usize, denominator: usize) -> Result<f64> {
    let max_index = values.len().saturating_sub(1);
    let rounded_index = max_index
        .checked_mul(numerator)
        .and_then(|value| value.checked_add(denominator / 2))
        .and_then(|value| value.checked_div(denominator))
        .ok_or_else(|| ServiceError::Internal("quantile index overflow".to_string()))?
        .min(max_index);
    values
        .get(rounded_index)
        .copied()
        .ok_or_else(|| ServiceError::Internal("quantile index out of range".to_string()))
}

/// Locate the histogram bucket for a value without using unchecked indexing.
fn histogram_bucket_index(value: f64, min: f64, width: f64, bucket_count: usize) -> Result<usize> {
    for index in 0..bucket_count {
        let next_index = index
            .checked_add(1)
            .ok_or_else(|| ServiceError::Internal("histogram bucket index overflow".to_string()))?;
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
        .ok_or_else(|| ServiceError::Internal("histogram bucket count invalid".to_string()))
}
