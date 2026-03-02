// Hokm/crates/hokm_core/src/deal.rs
//
// This module is responsible for:
// - creating a deck of 52 cards
// - shuffling it using a seed (deterministic shuffle)
// - dealing 13 cards to each of 4 players
//
// Why seed?
// Because:
// - debugging becomes easy (same seed = same deal)
// - server can store seed in logs
// - offline mode can reproduce games

use crate::{Card, Rank, Suit};

/// A small helper: list all suits in a stable order.
fn all_suits() -> [Suit; 4] {
    [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades]
}

/// A small helper: list all ranks in a stable order.
fn all_ranks() -> [Rank; 13] {
    [
        Rank::Two,
        Rank::Three,
        Rank::Four,
        Rank::Five,
        Rank::Six,
        Rank::Seven,
        Rank::Eight,
        Rank::Nine,
        Rank::Ten,
        Rank::Jack,
        Rank::Queen,
        Rank::King,
        Rank::Ace,
    ]
}

/// Build a fresh ordered 52-card deck.
///
/// Ordered deck is important because:
/// - if shuffle is deterministic, output is deterministic
/// - tests become predictable
pub fn build_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);

    for suit in all_suits() {
        for rank in all_ranks() {
            deck.push(Card { suit, rank });
        }
    }

    deck
}

/// Deterministic pseudo-random generator (simple).
///
/// This is NOT cryptographically secure.
/// It is good enough for:
/// - offline mode
/// - deterministic debug shuffles
///
/// For real online fairness, we can upgrade later
/// (server can use a secure RNG or commit-reveal).
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        // If seed is 0, xorshift becomes annoying. Fix it.
        let seed = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state: seed }
    }

    /// Get next random u64 (deterministic based on initial seed).
    fn next_u64(&mut self) -> u64 {
        // xorshift64*
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    /// Get a random number in range [0, n)
    fn gen_range(&mut self, n: usize) -> usize {
        // VERY IMPORTANT: n must not be 0
        (self.next_u64() as usize) % n
    }
}

/// Shuffle a deck in-place using Fisher-Yates shuffle.
///
/// Fisher-Yates is the standard correct shuffle:
/// - no bias (for a good RNG)
/// - simple
pub fn shuffle_in_place(deck: &mut [Card], seed: u64) {
    let mut rng = XorShift64::new(seed);

    // Go from the end to the beginning:
    // swap each card with a random earlier card (including itself).
    for i in (1..deck.len()).rev() {
        let j = rng.gen_range(i + 1);
        deck.swap(i, j);
    }
}

/// Deal 13 cards to each of 4 players.
/// Returns [hand0, hand1, hand2, hand3].
///
/// The deal order is:
/// card 0 -> player 0
/// card 1 -> player 1
/// card 2 -> player 2
/// card 3 -> player 3
/// card 4 -> player 0
/// ... etc
///
/// This matches typical dealing around the table.
pub fn deal_hands(seed: u64) -> [Vec<Card>; 4] {
    let mut deck = build_deck();
    shuffle_in_place(&mut deck, seed);

    // Create 4 empty hands with capacity 13 each
    let mut hands: [Vec<Card>; 4] = [
        Vec::with_capacity(13),
        Vec::with_capacity(13),
        Vec::with_capacity(13),
        Vec::with_capacity(13),
    ];

    // Distribute cards in round-robin
    for (i, card) in deck.into_iter().enumerate() {
        hands[i % 4].push(card);
    }

    // At the end each player has 13 cards
    hands
}

/// Optional: sort a hand for nicer UI.
/// Many people like to see suits grouped and ranks ordered.
/// This is NOT required for the rules to work.
///
/// You can call this in the client or server.
pub fn sort_hand(hand: &mut Vec<Card>) {
    // Define a suit order (you can change this if you want):
    // Clubs < Diamonds < Hearts < Spades
    fn suit_key(s: Suit) -> u8 {
        match s {
            Suit::Clubs => 0,
            Suit::Diamonds => 1,
            Suit::Hearts => 2,
            Suit::Spades => 3,
        }
    }

    // Sort by suit first, then by rank
    hand.sort_by_key(|c| (suit_key(c.suit), c.rank as u8));
}
