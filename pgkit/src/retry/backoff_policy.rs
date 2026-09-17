/// How the delay between attempts grows.
///
/// Defaults to [`Linear`](Self::Linear): a failing operation that exhausts its
/// retries does so without the long tail an exponential schedule would add.
#[derive(Debug, Clone, Default, Copy, PartialEq, Eq)]
pub enum BackoffPolicy {
    /// Every delay is the base delay.
    Fixed,
    /// Delay grows by the base delay each attempt: `base * attempt`.
    #[default]
    Linear,
    /// Delay doubles each attempt: `base * 2^(attempt - 1)`.
    Exponential,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_is_linear() {
        assert_eq!(BackoffPolicy::default(), BackoffPolicy::Linear);
    }
}
