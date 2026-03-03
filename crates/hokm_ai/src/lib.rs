#![forbid(unsafe_code)]

// Hokm/crates/hokm_ai/src/lib.rs
//
// Bots live here (NOT in the client).
// They only use hokm_core API:
// - GameState
// - legal_actions()
// - current_trick + hokm
//
// Difficulty levels:
// - Easy: random legal
// - Medium: cheap win else cheap
// - Hard: cheap win else cheap + partner-awareness (don’t overtake partner)

use hokm_core::{Action, Card, GameState, PlayerId, Suit};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Difficulty {
    Easy,
    Medium,
    Hard,
}

/// Pick a bot action at the requested difficulty.
/// Returns None if it is not bot's turn or no legal PlayCard exists.
pub fn choose_action(
    game: &GameState,
    bot: PlayerId,
    difficulty: Difficulty,
    seed: u64,
) -> Option<Action> {
    if game.turn != bot {
        return None;
    }
    if !matches!(game.phase, hokm_core::Phase::Playing) {
        return None;
    }

    // Extract legal play-cards (legal_actions already enforces follow-suit).
    let legal_cards: Vec<Card> = game
        .legal_actions(bot)
        .into_iter()
        .filter_map(|a| match a {
            Action::PlayCard { card } => Some(card),
            _ => None,
        })
        .collect();

    if legal_cards.is_empty() {
        return None;
    }

    match difficulty {
        Difficulty::Easy => {
            // Random legal (deterministic via seed)
            let idx = (XorShift64::new(seed).next_u64() as usize) % legal_cards.len();
            Some(Action::PlayCard {
                card: legal_cards[idx],
            })
        }
        Difficulty::Medium => Some(Action::PlayCard {
            card: medium_pick(game, bot, &legal_cards),
        }),
        Difficulty::Hard => Some(Action::PlayCard {
            card: hard_pick(game, bot, &legal_cards),
        }),
    }
}

// ------------------------------
// Medium bot: cheap win else cheap
// ------------------------------

fn medium_pick(game: &GameState, _bot: PlayerId, legal_cards: &[Card]) -> Card {
    // If leading: just cheapest legal.
    if game.current_trick.is_empty() {
        return cheapest_card(legal_cards, None);
    }

    let hokm = game.hokm.expect("hokm must exist in Playing");
    let lead = game.current_trick[0].1.suit;
    let (_cur_winner_pid, cur_winner_card) = current_winner(&game.current_trick, hokm);

    // Find all legal cards that beat current winning card.
    let winning: Vec<Card> = legal_cards
        .iter()
        .copied()
        .filter(|c| beats(*c, cur_winner_card, lead, hokm))
        .collect();

    if !winning.is_empty() {
        // Use cheapest winning card (don’t waste big cards).
        return cheapest_card(&winning, Some(hokm));
    }

    // Can't win => throw cheapest.
    cheapest_card(legal_cards, Some(hokm))
}

// ------------------------------
// Hard bot: adds partner awareness
// ------------------------------

fn hard_pick(game: &GameState, bot: PlayerId, legal_cards: &[Card]) -> Card {
    let hokm = game.hokm.expect("hokm must exist in Playing");
    let partner = PlayerId((bot.0 + 2) % 4);

    // If leading: try to lead low non-trump if possible (save trump).
    if game.current_trick.is_empty() {
        // Prefer non-trump cards, cheapest first.
        let non_trump: Vec<Card> = legal_cards
            .iter()
            .copied()
            .filter(|c| c.suit != hokm)
            .collect();
        if !non_trump.is_empty() {
            return cheapest_card(&non_trump, Some(hokm));
        }
        return cheapest_card(legal_cards, Some(hokm));
    }

    let lead = game.current_trick[0].1.suit;
    let (cur_winner_pid, cur_winner_card) = current_winner(&game.current_trick, hokm);

    // If partner is currently winning the trick, DON'T overtake them unless you must.
    // Most of the time, you should throw a cheap card and keep power for later.
    if cur_winner_pid == partner {
        return cheapest_card(legal_cards, Some(hokm));
    }

    // Opponent is winning -> try to win cheaply.
    let winning: Vec<Card> = legal_cards
        .iter()
        .copied()
        .filter(|c| beats(*c, cur_winner_card, lead, hokm))
        .collect();

    if !winning.is_empty() {
        // Prefer a winning card that is NOT trump if possible (save trump).
        let winning_non_trump: Vec<Card> =
            winning.iter().copied().filter(|c| c.suit != hokm).collect();
        if !winning_non_trump.is_empty() {
            return cheapest_card(&winning_non_trump, Some(hokm));
        }

        // Otherwise win with cheapest trump.
        return cheapest_card(&winning, Some(hokm));
    }

    // Can't win -> throw cheap.
    cheapest_card(legal_cards, Some(hokm))
}

// ------------------------------
// Trick helpers
// ------------------------------

fn current_winner(trick: &[(PlayerId, Card)], hokm: Suit) -> (PlayerId, Card) {
    let lead = trick[0].1.suit;
    let mut best = trick[0];

    for &(pid, card) in trick.iter().skip(1) {
        if beats(card, best.1, lead, hokm) {
            best = (pid, card);
        }
    }
    best
}

fn beats(a: Card, b: Card, lead: Suit, hokm: Suit) -> bool {
    let a_trump = a.suit == hokm;
    let b_trump = b.suit == hokm;

    if a_trump && !b_trump {
        return true;
    }
    if !a_trump && b_trump {
        return false;
    }
    if a_trump && b_trump {
        return a.rank > b.rank;
    }

    // neither trump -> only lead suit competes
    let a_lead = a.suit == lead;
    let b_lead = b.suit == lead;
    match (a_lead, b_lead) {
        (true, false) => true,
        (false, true) => false,
        (true, true) => a.rank > b.rank,
        (false, false) => false,
    }
}

// Pick a cheap card (tries to avoid trump if you pass hokm)
fn cheapest_card(cards: &[Card], hokm: Option<Suit>) -> Card {
    fn suit_key(s: Suit) -> u8 {
        match s {
            Suit::Clubs => 0,
            Suit::Diamonds => 1,
            Suit::Hearts => 2,
            Suit::Spades => 3,
        }
    }

    // Sort key:
    // - if hokm provided: non-trump first (0) then trump (1)
    // - then low rank
    // - then suit key for tie-break
    let hk = hokm;
    *cards
        .iter()
        .min_by_key(|c| {
            let trump_penalty = if hk.is_some() && c.suit == hk.unwrap() {
                1u8
            } else {
                0u8
            };
            (trump_penalty, c.rank as u8, suit_key(c.suit))
        })
        .expect("cards not empty")
}

// Tiny deterministic RNG (same as core idea, no external crate needed)
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
}
