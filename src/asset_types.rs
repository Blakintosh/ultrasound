use crate::units::linear_to_db_spl;

pub struct AssetEnvelope {
    pub left: Vec<f64>,
    pub time: Vec<f64>,
}

const ENVELOPE_BUCKET_SIZE: usize = 512;

impl AssetEnvelope {
    pub fn envelope_extract(
        frame_count: u64,
        channel_count: u16,
        samples: &[i16],
    ) -> AssetEnvelope {
        let bucket_count = (frame_count / ENVELOPE_BUCKET_SIZE as u64) as usize;

        // If at or below bucket size, return a more basic envelope.
        if bucket_count <= 1 {
            let peak =
                samples.iter().map(|&x| x.unsigned_abs()).max().unwrap_or(0) as f64 / 32768.0;

            return AssetEnvelope {
                left: vec![peak, peak, peak, peak],
                time: vec![0.0, 0.3, 0.6, 1.0],
            };
        }

        let mut buckets = vec![0f64; bucket_count];
        let mut smoothed = vec![0f64; bucket_count];

        // Compute peak amplitude per bucket, then convert to dB SPL.
        for i in 0..bucket_count {
            let base_offset = i * ENVELOPE_BUCKET_SIZE;

            for j in 0..channel_count as usize {
                for k in 0..ENVELOPE_BUCKET_SIZE {
                    let read_offset = (base_offset + k) * channel_count as usize + j;
                    buckets[i] = buckets[i].max((samples[read_offset] as f64).abs());
                }
            }

            buckets[i] = linear_to_db_spl(buckets[i] / 32768.0);
        }

        // Iteratively find local maxima and decimate until we have < 32 peaks.
        let mut maxima: Vec<usize>;
        loop {
            maxima = Vec::new();
            maxima.push(0);

            let mut i = 1;
            while i < bucket_count - 1 {
                // Walk up
                while buckets[i - 1] <= buckets[i] {
                    i += 1;
                    if i >= bucket_count - 1 {
                        break;
                    }
                }
                add_maxima(&buckets, &mut maxima, i - 1);

                // Walk down
                while i < bucket_count - 1 && buckets[i - 1] >= buckets[i] {
                    i += 1;
                    if i >= bucket_count - 1 {
                        break;
                    }
                }

                i += 1;
            }

            maxima.push(bucket_count - 1);

            if maxima.len() < 32 {
                break;
            }

            // Too many maxima — interpolate between them and retry.
            let mut prev = maxima[0];
            maxima.remove(0);

            for i in 0..bucket_count {
                if maxima[0] < i {
                    prev = maxima[0];
                    maxima.remove(0);
                }
                let left_idx = prev;
                let right_idx = maxima[0];

                if right_idx == left_idx {
                    smoothed[left_idx] = buckets[left_idx];
                } else {
                    let t = (i - left_idx) as f64 / (right_idx - left_idx) as f64;
                    smoothed[i] =
                        (buckets[left_idx] * (1.0 - t) + buckets[right_idx] * t) as i32 as f64;
                }
            }

            buckets = smoothed.clone();
        }

        // Generate candidate 4-point envelopes and pick the best fit.
        let mut candidates: Vec<AssetEnvelope> = Vec::new();

        if maxima.len() <= 4 {
            let idx1 = maxima.len() / 2 - 1;
            let idx2 = maxima.len() / 2;

            candidates.push(AssetEnvelope {
                left: vec![
                    buckets[maxima[0]],
                    buckets[maxima[idx1]],
                    buckets[maxima[idx2]],
                    buckets[*maxima.last().unwrap()],
                ],
                time: vec![
                    0.0,
                    maxima[idx1] as f64 / bucket_count as f64,
                    maxima[idx2] as f64 / bucket_count as f64,
                    1.0,
                ],
            });
        } else {
            for i in 1..maxima.len() - 2 {
                for j in (i + 1)..maxima.len() - 2 {
                    candidates.push(AssetEnvelope {
                        left: vec![
                            buckets[maxima[0]],
                            buckets[maxima[i]],
                            buckets[maxima[j]],
                            buckets[*maxima.last().unwrap()],
                        ],
                        time: vec![
                            0.0,
                            maxima[i] as f64 / bucket_count as f64,
                            maxima[j] as f64 / bucket_count as f64,
                            1.0,
                        ],
                    });
                }
            }
        }

        // Score each candidate and return the best.
        let mut best_idx = 0;
        let mut best_score = f64::MAX;
        for (idx, candidate) in candidates.iter().enumerate() {
            let score = candidate.eval(&buckets);
            if score < best_score {
                best_score = score;
                best_idx = idx;
            }
        }

        candidates.swap_remove(best_idx)
    }

    /// Mean squared error of this envelope approximation against the actual buckets.
    fn eval(&self, buckets: &[f64]) -> f64 {
        let points: [usize; 4] = [
            0,
            (self.time[1] * (buckets.len() - 1) as f64) as usize,
            (self.time[2] * (buckets.len() - 1) as f64) as usize,
            buckets.len() - 1,
        ];

        let mut error = 0.0;
        for seg in 0..3 {
            for i in points[seg]..points[seg + 1] {
                let span = points[seg + 1] - points[seg];
                if span == 0 {
                    continue;
                }
                let t = (i - points[seg]) as f64 / span as f64;
                let interpolated = self.time[seg] * (1.0 - t) + self.time[seg + 1] * t;
                error += (interpolated - buckets[i]).powi(2);
            }
        }

        error / buckets.len() as f64
    }
}

/// Deduplicates consecutive maxima with equal bucket values, then appends new index.
fn add_maxima(buckets: &[f64], maxima: &mut Vec<usize>, i: usize) {
    if maxima.len() >= 2 {
        let prev2 = buckets[maxima[maxima.len() - 2]];
        let prev1 = buckets[maxima[maxima.len() - 1]];
        if prev2 == prev1 && prev1 == buckets[i] {
            maxima.remove(maxima.len() - 1);
        }
    }
    maxima.push(i);
}
