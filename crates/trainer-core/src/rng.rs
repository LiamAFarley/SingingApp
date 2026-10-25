//! Small deterministic PRNG (xoshiro256**) so that trial generation from a seed never changes
//! when dependencies are upgraded.

#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        let mut st = seed;
        let s = [
            splitmix64(&mut st),
            splitmix64(&mut st),
            splitmix64(&mut st),
            splitmix64(&mut st),
        ];
        Rng { s }
    }

    /// Derives an independent stream, e.g. one per block.
    pub fn derive(seed: u64, stream: u64) -> Self {
        let mut st = seed ^ stream.wrapping_mul(0xA24B_AED4_963E_E407);
        Rng::new(splitmix64(&mut st))
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform in [0, 1).
    pub fn f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform integer in [0, n). Returns 0 when n == 0.
    pub fn below(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        // Lemire's multiply-shift; the tiny bias is irrelevant for exercise generation.
        ((self.next_u64() as u128 * n as u128) >> 64) as usize
    }

    /// Uniform integer in [lo, hi] (inclusive).
    pub fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + self.below((hi - lo + 1) as usize) as i32
    }

    pub fn range_f64(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.f64()
    }

    pub fn chance(&mut self, p: f64) -> bool {
        self.f64() < p
    }

    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            None
        } else {
            Some(&items[self.below(items.len())])
        }
    }

    pub fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = self.below(i + 1);
            items.swap(i, j);
        }
    }
}

/// Draws items so that every item appears equally often over each full cycle, without
/// repeating the same item twice in a row across cycle boundaries.
pub struct BalancedBag<T: Clone + PartialEq> {
    items: Vec<T>,
    bag: Vec<T>,
    last: Option<T>,
    /// A new cycle never starts with the item just drawn.
    strict: bool,
}

/// Cards per cycle for `BalancedBag::allowing_repeats`.
pub const REPEAT_CYCLE: usize = 24;

impl<T: Clone + PartialEq> BalancedBag<T> {
    /// Every item once per cycle, never the same item twice in a row.
    pub fn new(items: Vec<T>) -> Self {
        BalancedBag {
            items,
            bag: Vec::new(),
            last: None,
            strict: true,
        }
    }

    /// Every item equally often, but with several copies per cycle (at least `REPEAT_CYCLE`
    /// cards), so an item can come up twice in a row. With one copy each, a small set would
    /// alternate and the next answer could be guessed from the last.
    pub fn allowing_repeats(items: Vec<T>) -> Self {
        let copies = REPEAT_CYCLE.div_ceil(items.len().max(1));
        let items = items
            .iter()
            .flat_map(|x| std::iter::repeat_n(x.clone(), copies))
            .collect();
        BalancedBag {
            items,
            bag: Vec::new(),
            last: None,
            strict: false,
        }
    }

    pub fn draw(&mut self, rng: &mut Rng) -> Option<T> {
        if self.items.is_empty() {
            return None;
        }
        self.refill_if_empty(rng);
        let item = self.bag.pop();
        self.last = item.clone();
        item
    }

    /// Starts the next shuffled cycle when this one is used up.
    pub fn refill_if_empty(&mut self, rng: &mut Rng) {
        if self.bag.is_empty() {
            self.bag = self.items.clone();
            rng.shuffle(&mut self.bag);
            if self.strict && self.bag.len() > 1 && self.bag.last() == self.last.as_ref() {
                let n = self.bag.len();
                self.bag.swap(n - 1, 0);
            }
        }
    }

    /// Returns a drawn item that was not used, at a random place among those still to come, so
    /// the counts stay balanced.
    pub fn put_back(&mut self, item: T, rng: &mut Rng) {
        let at = rng.below(self.bag.len() + 1);
        self.bag.insert(at, item);
    }

    /// Takes one `item` out of those still to come, if it is there, because it was used in place
    /// of a drawn one.
    pub fn take(&mut self, item: &T) -> bool {
        match self.bag.iter().position(|x| x == item) {
            Some(i) => {
                self.bag.remove(i);
                true
            }
            None => false,
        }
    }

    pub fn pending(&self, item: &T) -> bool {
        self.bag.contains(item)
    }
}

/// Draws items in proportion to their weights, one cycle at a time. Each cycle holds every item
/// as many times as its share of the cycle, with the fractions carried into later cycles, then
/// shuffled, so the long-run shares are exact and long streaks of one item cannot happen.
pub struct WeightedBag<T: Clone + PartialEq> {
    items: Vec<(T, f64)>,
    carry: Vec<f64>,
    cycle: f64,
    bag: Vec<T>,
    last: Option<T>,
}

impl<T: Clone + PartialEq> WeightedBag<T> {
    /// `weights` need not sum to one; `cycle` is the number of draws per cycle.
    pub fn new(weights: Vec<(T, f64)>, cycle: usize) -> Self {
        let total: f64 = weights.iter().map(|w| w.1.max(0.0)).sum::<f64>().max(1e-12);
        let items: Vec<(T, f64)> = weights
            .into_iter()
            .map(|(t, w)| (t, w.max(0.0) / total))
            .collect();
        WeightedBag {
            carry: vec![0.0; items.len()],
            items,
            cycle: cycle.max(1) as f64,
            bag: Vec::new(),
            last: None,
        }
    }

    pub fn draw(&mut self, rng: &mut Rng) -> Option<T> {
        if self.items.is_empty() {
            return None;
        }
        while self.bag.is_empty() {
            for (i, (item, w)) in self.items.iter().enumerate() {
                self.carry[i] += w * self.cycle;
                let k = self.carry[i].floor();
                self.carry[i] -= k;
                for _ in 0..k as usize {
                    self.bag.push(item.clone());
                }
            }
            rng.shuffle(&mut self.bag);
            if self.bag.len() > 1 && self.bag.last() == self.last.as_ref() {
                if let Some(j) = self.bag.iter().position(|x| Some(x) != self.last.as_ref()) {
                    let n = self.bag.len();
                    self.bag.swap(n - 1, j);
                }
            }
        }
        let item = self.bag.pop();
        self.last = item.clone();
        item
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_sequence() {
        let mut a = Rng::new(42);
        let mut b = Rng::new(42);
        for _ in 0..100 {
            assert_eq!(a.next_u64(), b.next_u64());
        }
        assert_ne!(Rng::new(1).next_u64(), Rng::new(2).next_u64());
        assert_ne!(Rng::derive(1, 0).next_u64(), Rng::derive(1, 1).next_u64());
    }

    #[test]
    fn sequence_is_pinned() {
        // Stored sessions are regenerated from their seeds, so the generator must never change.
        let mut r = Rng::new(7);
        let v: Vec<u64> = (0..3).map(|_| r.next_u64()).collect();
        assert_eq!(v, PINNED_SEED_7);
    }

    const PINNED_SEED_7: [u64; 3] = [
        12923355070828475994,
        5142052590334782674,
        15488392906492639638,
    ];

    #[test]
    fn ranges_stay_in_bounds() {
        let mut r = Rng::new(3);
        for _ in 0..10_000 {
            let v = r.range_i32(-3, 4);
            assert!((-3..=4).contains(&v));
            let f = r.f64();
            assert!((0.0..1.0).contains(&f));
        }
    }

    #[test]
    fn balanced_bag_is_balanced_and_avoids_repeats() {
        let mut r = Rng::new(9);
        let mut bag = BalancedBag::new(vec![0, 1, 2]);
        let draws: Vec<i32> = (0..300).map(|_| bag.draw(&mut r).unwrap()).collect();
        for k in 0..3 {
            assert_eq!(draws.iter().filter(|&&d| d == k).count(), 100);
        }
        assert!(draws.windows(2).all(|w| w[0] != w[1]));
    }

    #[test]
    fn weighted_bag_matches_the_weights() {
        let mut r = Rng::new(11);
        // Brady's first stage: C on 40% of trials, the other eleven sharing the rest.
        let mut weights = vec![(0usize, 0.40)];
        weights.extend((1..12).map(|k| (k, 0.60 / 11.0)));
        let mut bag = WeightedBag::new(weights, 24);
        let n = 12_000;
        let draws: Vec<usize> = (0..n).map(|_| bag.draw(&mut r).unwrap()).collect();
        let share = |k: usize| draws.iter().filter(|&&d| d == k).count() as f64 / n as f64;
        assert!((share(0) - 0.40).abs() < 0.02, "{}", share(0));
        for k in 1..12 {
            assert!((share(k) - 0.60 / 11.0).abs() < 0.02, "{k}: {}", share(k));
        }
        // No long streaks of the heavy class.
        let longest = draws
            .chunk_by(|a, b| a == b)
            .map(|c| c.len())
            .max()
            .unwrap();
        assert!(longest <= 10, "{longest}");
    }
}
