//! Sampling: how many merges of a batch to look at closely, and which ones. The choice is random, never weighted
//! by risk, and the caller keeps the chosen set so it can be recorded. It is never given to the worker.

use super::Sampling;

/// How many of `n` merges to pick: a quarter, rounded up, or none when sampling is off.
pub fn plan_size(sampling: Sampling, n: u32) -> u32 {
    match sampling {
        Sampling::Off => 0,
        Sampling::OneInFour => n.div_ceil(4),
    }
}

/// A small seeded random source (splitmix64). The same seed always gives the same sequence. A caller draws the
/// seed from the system (`getrandom`) and records it with the chosen set.
#[derive(Clone, Debug)]
pub struct Seeded(u64);

impl Seeded {
    pub fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number in `0..bound` (`bound` above zero); the tiny modulo bias is harmless for a sample of merges.
    fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }
}

/// Picks `plan` distinct items of `batch` at random, in the order they were drawn. Asking for more than the batch
/// holds returns the whole batch. The same batch, plan and seed give the same answer.
pub fn choose<T: Clone>(batch: &[T], plan: u32, rng: &mut Seeded) -> Vec<T> {
    let want = (plan as usize).min(batch.len());
    let mut pool: Vec<usize> = (0..batch.len()).collect();
    let mut chosen = Vec::with_capacity(want);
    for _ in 0..want {
        let at = rng.below(pool.len());
        chosen.push(batch[pool.swap_remove(at)].clone());
    }
    chosen
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;

    #[test]
    fn sampling_is_off_by_default_and_plans_nothing() {
        assert_eq!(Sampling::default(), Sampling::Off);
        assert_eq!(plan_size(Sampling::default(), 40), 0);
    }

    #[test]
    fn one_in_four_rounds_up() {
        let sizes: Vec<u32> = [0, 1, 4, 5, 8, 9]
            .iter()
            .map(|n| plan_size(Sampling::OneInFour, *n))
            .collect();
        assert_eq!(sizes, [0, 1, 1, 2, 2, 3]);
    }

    #[test]
    fn choose_returns_exactly_the_plan_of_distinct_items_from_the_batch() {
        let batch: Vec<u32> = (100..120).collect();
        let plan = plan_size(Sampling::OneInFour, 20);
        for seed in 0..50 {
            let got = choose(&batch, plan, &mut Seeded::new(seed));
            assert_eq!(got.len(), plan as usize);
            let set: BTreeSet<_> = got.iter().collect();
            assert_eq!(set.len(), got.len());
            assert!(got.iter().all(|x| batch.contains(x)));
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_choice() {
        let batch: Vec<u32> = (0..30).collect();
        let a = choose(&batch, 8, &mut Seeded::new(7));
        let b = choose(&batch, 8, &mut Seeded::new(7));
        assert_eq!(a, b);
        assert!((0..20).any(|s| choose(&batch, 8, &mut Seeded::new(s)) != a));
    }

    #[test]
    fn an_empty_batch_or_a_plan_of_zero_chooses_nothing() {
        let none: [u32; 0] = [];
        assert!(choose(&none, 3, &mut Seeded::new(1)).is_empty());
        assert!(choose(&[1, 2, 3], 0, &mut Seeded::new(1)).is_empty());
    }

    #[test]
    fn a_plan_larger_than_the_batch_returns_the_whole_batch_once() {
        let mut got = choose(&[1, 2, 3], 10, &mut Seeded::new(3));
        got.sort_unstable();
        assert_eq!(got, [1, 2, 3]);
    }
}
