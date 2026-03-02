// Hokm/crates/hokm_client/src/main.rs
//
// Right now this is ONLY a test program:
// - deal real hands
// - (optionally) sort them
// - create a GameState
// - choose hokm
// - print a few checks
//
// Later we will replace this with a real TUI / GUI loop.

use hokm_core::{deal_hands, sort_hand, Action, GameState, PlayerId, Suit};

fn main() {
    println!("hokm_client: started ✅");

    // Seed gives deterministic deal:
    // same seed -> same exact hands (very good for debugging)
    let seed: u64 = 12345;

    // Deal 13 cards per player
    let mut hands = deal_hands(seed);

    // Sort hands for nicer printing (optional)
    for h in hands.iter_mut() {
        sort_hand(h);
    }

    // Dealer is player 0 for now
    let mut game = GameState::new_with_hands(PlayerId(0), hands);

    // Dealer chooses hokm (trump suit)
    let ev = game
        .apply_action(PlayerId(0), Action::ChooseHokm { suit: Suit::Spades })
        .expect("dealer should be able to choose hokm");

    println!("Events after choosing hokm: {:?}", ev);

    // Quick sanity checks
    println!("Player 0 has {} cards", game.hands[0].len());
    println!("Player 1 has {} cards", game.hands[1].len());
    println!("Player 2 has {} cards", game.hands[2].len());
    println!("Player 3 has {} cards", game.hands[3].len());

    println!(
        "Now phase is: {:?}, turn is player {}",
        game.phase, game.turn.0
    );
}
