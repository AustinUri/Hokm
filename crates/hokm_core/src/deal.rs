// Hokm/crates/hokm_core/src/deal.rs
//
// This module handles:
// - creating a deck
// - shuffling with a seed (deterministic)
// - dealing hands
// - choosing Hokm chooser by "first Ace draw"
//
// IMPORTANT RULES (your variant):
// 1) Before the real deal, we do a pre-draw to find the chooser:
//    - Reveal cards round-robin to players 0,1,2,3,0,1...
//    - First Ace that appears => that player is the Hokm chooser.
// 2) Dealer is the player right BEFORE chooser (so chooser is left of dealer).
// 3) After chooser is found, we deal AGAIN for the real round.
// 4) Real dealing starts from left of dealer (dealer.next()).

use crate::{Card, PlayerId, Rank, Suit};

fn all_suits() -> [Suit; 4] {
    [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades]
}

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

/// Build an ordered 52-card deck.
pub fn build_deck() -> Vec<Card> {
    let mut deck = Vec::with_capacity(52);
    for suit in all_suits() {
        for rank in all_ranks() {
            deck.push(Card { suit, rank });
        }
    }
    deck
}

/// Tiny deterministic RNG (not crypto-secure).
struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        let seed = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn gen_range(&mut self, n: usize) -> usize {
        (self.next_u64() as usize) % n
    }
}

/// Fisher-Yates shuffle (correct shuffle) using deterministic RNG.
pub fn shuffle_in_place(deck: &mut [Card], seed: u64) {
    let mut rng = XorShift64::new(seed);
    for i in (1..deck.len()).rev() {
        let j = rng.gen_range(i + 1);
        deck.swap(i, j);
    }
}

/// Your setup rule:
/// Reveal cards to players in order 0,1,2,3,0,1...
/// First ACE that appears => that player is the chooser.
///
/// Returns: (chooser, ace_card_that_was_drawn)
pub fn chooser_by_first_ace(seed: u64) -> (PlayerId, Card) {
    let mut deck = build_deck();
    shuffle_in_place(&mut deck, seed);

    for (i, card) in deck.into_iter().enumerate() {
        let pid = PlayerId((i % 4) as u8);
        if card.rank == Rank::Ace {
            return (pid, card);
        }
    }

    // A 52-card deck always contains aces, so this can't happen.
    panic!("No Ace found in deck (impossible)");
}

/// Deal 13 cards each, BUT:
/// The FIRST card goes to the player left of the dealer (dealer.next()).
///
/// Example:
/// - dealer=0 => first card to 1
/// - dealer=2 => first card to 3
pub fn deal_hands_with_dealer(seed: u64, dealer: PlayerId) -> [Vec<Card>; 4] {
    let mut deck = build_deck();
    shuffle_in_place(&mut deck, seed);

    let mut hands: [Vec<Card>; 4] = [
        Vec::with_capacity(13),
        Vec::with_capacity(13),
        Vec::with_capacity(13),
        Vec::with_capacity(13),
    ];

    let start = dealer.next().0 as usize;

    for (i, card) in deck.into_iter().enumerate() {
        let pid = (start + (i % 4)) % 4;
        hands[pid].push(card);
    }

    hands
}

/// Convenience: old behavior wrapper.
/// This makes the FIRST card go to player 0 by pretending dealer is 3 (since 3.next()=0).
pub fn deal_hands(seed: u64) -> [Vec<Card>; 4] {
    deal_hands_with_dealer(seed, PlayerId(3))
}

/// Optional hand sorting for nicer UI.
/// Does NOT affect game rules.
pub fn sort_hand(hand: &mut Vec<Card>) {
    fn suit_key(s: Suit) -> u8 {
        match s {
            Suit::Clubs => 0,
            Suit::Diamonds => 1,
            Suit::Hearts => 2,
            Suit::Spades => 3,
        }
    }
    hand.sort_by_key(|c| (suit_key(c.suit), c.rank as u8));
}
