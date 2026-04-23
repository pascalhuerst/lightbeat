//! Burg's method for estimating linear prediction (autoregressive) coefficients.
//!
//! Given a signal `x[0..N]`, computes coefficients `a[1..=p]` such that
//! `x̂[n] = Σ a[k] · x[n − k]` (eqn. 11 in the paper) minimises the average
//! of forward and backward prediction errors.
//!
//! The recursion is the standard Burg update (Algorithm 2 in the paper, with
//! the denominator corrected from `f·f + f·f` to `f·f + b·b`, which is the
//! canonical formulation — see e.g. Makhoul 1975).

/// Compute `order` linear prediction coefficients for `signal` using Burg's method.
///
/// Returns a vector of length `order` holding `[a_1, a_2, …, a_order]`.
/// If the signal is too short (`len <= order`) or all-zero, returns zeros.
pub fn burg(signal: &[f64], order: usize) -> Vec<f64> {
    let n = signal.len();
    if order == 0 || n <= order {
        return vec![0.0; order];
    }

    // Forward and backward error buffers.
    let mut f: Vec<f64> = signal.to_vec();
    let mut b: Vec<f64> = signal.to_vec();
    // Coefficients (polynomial form). a[0] is the leading 1 of the AR polynomial.
    let mut a: Vec<f64> = vec![0.0; order + 1];
    a[0] = 1.0;

    for m in 0..order {
        // f_p = f without its first element; b_p = b without its last element.
        // Equivalently we work with the slices f[1..] and b[..len-1].
        let mut num = 0.0;
        let mut den = 0.0;
        let len = f.len();
        // Burg reflection coefficient.
        for i in 1..len {
            num += f[i] * b[i - 1];
            den += f[i] * f[i] + b[i - 1] * b[i - 1];
        }
        let k = if den.abs() > f64::EPSILON {
            -2.0 * num / den
        } else {
            0.0
        };

        // Update forward and backward errors in place. Read old values first:
        // the update reads f_p and b_p which are f[1..] and b[..len-1].
        let mut new_f = vec![0.0; len - 1];
        let mut new_b = vec![0.0; len - 1];
        for i in 0..len - 1 {
            new_f[i] = f[i + 1] + k * b[i];
            new_b[i] = b[i] + k * f[i + 1];
        }
        f = new_f;
        b = new_b;

        // a_new[i] = a[i] + k * a[m - i + 1]_reversed. In the paper:
        //   a ← (a[0], a[1], …, a[m], 0) + k · (0, a[m], a[m-1], …, a[0])
        // Apply symmetrically using a temporary snapshot of the first m+1 entries.
        let snap: Vec<f64> = a[..=m].to_vec();
        for i in 0..=m + 1 {
            let left = if i <= m { snap[i] } else { 0.0 };
            let right = if i == 0 { 0.0 } else { snap[m + 1 - i] };
            a[i] = left + k * right;
        }
    }

    // Drop the leading 1; return [a_1, …, a_order] in the convention of eqn. 11.
    // Paper's prediction is x̂[n] = Σ a_k x[n-k], but the AR polynomial convention
    // is x[n] + Σ c_k x[n-k] = e[n]. So a_k = -c_k.
    a[1..].iter().map(|c| -c).collect()
}

/// Predict the next sample given `order` history samples (most recent last) and
/// previously-computed coefficients.
///
/// Eqn. 11 from the paper: `x̂[n] = Σ_{k=1..=p} a[k] · x[n − k]`.
pub fn predict_next(history: &[f64], coeffs: &[f64]) -> f64 {
    debug_assert_eq!(history.len(), coeffs.len());
    // history[last] is x[n-1], history[last-1] is x[n-2], etc.
    let mut y = 0.0;
    let n = history.len();
    for k in 0..n {
        y += coeffs[k] * history[n - 1 - k];
    }
    y
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_order_returns_empty() {
        let a = burg(&[1.0, 2.0, 3.0, 4.0], 0);
        assert!(a.is_empty());
    }

    #[test]
    fn short_signal_returns_zeros() {
        let a = burg(&[1.0, 2.0], 5);
        assert_eq!(a, vec![0.0; 5]);
    }

    #[test]
    fn predicts_linear_ramp_well() {
        // A pure linear ramp has an AR(1) model x[n] ≈ 2x[n-1] − x[n-2], so
        // an order-2 predictor should nail it.
        let signal: Vec<f64> = (0..64).map(|i| i as f64).collect();
        let coeffs = burg(&signal, 2);
        let history: Vec<f64> = signal[signal.len() - 2..].to_vec();
        let pred = predict_next(&history, &coeffs);
        let actual = signal.len() as f64;
        assert!((pred - actual).abs() < 1.0, "predicted {pred}, expected {actual}");
    }

    #[test]
    fn predicts_sinusoid() {
        // A pure sinusoid admits an exact AR(2) model. Burg should recover it.
        let f = 0.05; // cycles/sample
        let signal: Vec<f64> = (0..128)
            .map(|n| (2.0 * std::f64::consts::PI * f * n as f64).sin())
            .collect();
        let coeffs = burg(&signal, 2);
        let history: Vec<f64> = signal[signal.len() - 2..].to_vec();
        let pred = predict_next(&history, &coeffs);
        let actual = (2.0 * std::f64::consts::PI * f * signal.len() as f64).sin();
        assert!((pred - actual).abs() < 0.05, "predicted {pred}, expected {actual}");
    }
}
