// Hokm/crates/hokm_client/src/main.rs
//
// Simple CLI Hokm:
// - pick bot difficulty
// - generate a RANDOM seed every run (different cards each time)
// - deal
// - show first 5 dealt cards (before sorting)
// - choose hokm
// - play a full round (until a team wins 7 tricks)

use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use hokm_ai::Difficulty;
use hokm_core::{deal_hands, sort_hand, Action, Card, GameState, Phase, PlayerId, Suit};

fn main() {
    println!("hokm_client: started ✅");

    // 1) Choose global difficulty for bots
    let bot_diff = ask_difficulty();
    println!("Bot difficulty set to: {:?}\n", bot_diff);

    // 2) Random seed each run -> different cards each run
    // We still PRINT it so you can copy it if you want to replay the same deal later.
    let seed: u64 = random_seed();
    println!("Random seed this run: {}\n", seed);

    // 3) Deal hands in deal-order (DON'T sort yet)
    let mut hands = deal_hands(seed);

    // For now dealer is always player 0 in this single-round demo
    let dealer = PlayerId(0);

    // 4) Show first 5 cards dealt to dealer BEFORE sorting
    println!("First 5 cards you got (deal order):");
    for i in 0..5 {
        let c = hands[dealer.idx()][i];
        println!("  {}", card_to_string(c));
    }
    println!();

    // 5) Sort for nicer printing (optional)
    for h in hands.iter_mut() {
        sort_hand(h);
    }

    // 6) Create game + choose hokm
    let mut game = GameState::new_with_hands(dealer, hands);

    let hokm = ask_hokm_suit();
    let events = game
        .apply_action(dealer, Action::ChooseHokm { suit: hokm })
        .expect("dealer should be able to choose hokm");
    println!("Events: {:?}\n", events);

    println!("--- ROUND START ---");
    println!("Dealer: Player {}", game.dealer.0);
    println!("Hokm:   {:?}\n", hokm);

    // 7) Play full round until RoundOver
    play_round(&mut game, bot_diff, seed);

    // 8) Round result
    match game.phase {
        Phase::RoundOver { winner, kot } => {
            println!("\n🏁 ROUND OVER");
            println!("Winner team: {:?} (A=0&2, B=1&3)", winner);
            println!("Kot: {}", kot);
            println!(
                "Final tricks: Team A = {}, Team B = {}",
                game.tricks_taken[0], game.tricks_taken[1]
            );
        }
        Phase::GameOver { winner } => {
            println!("\n🏆 GAME OVER");
            println!("Winner team: {:?}", winner);
        }
        _ => println!("Weird: round didn't end but program ended."),
    }
}

// ----------------------------
// Round / Trick loops
// ----------------------------

fn play_round(game: &mut GameState, bot_diff: Difficulty, seed: u64) {
    let mut trick_number: u32 = 1;

    loop {
        if matches!(game.phase, Phase::RoundOver { .. } | Phase::GameOver { .. }) {
            break;
        }

        println!("\n=== Trick {} ===", trick_number);

        // change seed a bit per trick (so Easy random bot doesn't repeat patterns)
        play_one_trick(
            game,
            bot_diff,
            seed.wrapping_add(trick_number as u64 * 9999),
        );

        trick_number += 1;

        println!(
            "Tricks taken so far: Team A = {}, Team B = {}",
            game.tricks_taken[0], game.tricks_taken[1]
        );
    }
}

fn play_one_trick(game: &mut GameState, bot_diff: Difficulty, seed: u64) {
    let mut trick_started = false;

    loop {
        if trick_started && game.current_trick.is_empty() {
            return;
        }
        if !game.current_trick.is_empty() {
            trick_started = true;
        }

        print_public_state(game);

        let pid = game.turn;

        if pid.0 == 0 {
            play_human_card(game, pid);
        } else {
            play_bot(game, pid, bot_diff, seed.wrapping_add(pid.0 as u64 * 1234));
        }
    }
}

// ----------------------------
// Bot + Human play
// ----------------------------

fn play_bot(game: &mut GameState, pid: PlayerId, diff: Difficulty, seed: u64) {
    let action = hokm_ai::choose_action(game, pid, diff, seed).expect("bot had no action");

    let hokm_core::Action::PlayCard { card } = action else {
        panic!("bot returned non-PlayCard (bug)");
    };

    println!("Bot P{} plays {}", pid.0, card_to_string(card));

    let events = game
        .apply_action(pid, action)
        .expect("bot played illegal move (bug)");
    if let Some(w) = trick_winner_from_events(&events) {
        println!("✅ Trick winner: Player {}\n", w.0);
    } else {
        println!("Events: {:?}\n", events);
    }
}

fn play_human_card(game: &mut GameState, pid: PlayerId) {
    loop {
        let legal_cards: Vec<Card> = game
            .legal_actions(pid)
            .iter()
            .filter_map(|a| match a {
                Action::PlayCard { card } => Some(*card),
                _ => None,
            })
            .collect();

        print!("Legal cards: ");
        for c in &legal_cards {
            print!("{} ", card_to_string(*c));
        }
        println!();

        print!("play index> ");
        flush();

        let input = read_line_trimmed();
        let Ok(index) = input.parse::<usize>() else {
            println!("Type a number index (0,1,2...).\n");
            continue;
        };

        let hand = &game.hands[pid.idx()];
        if index >= hand.len() {
            println!("Bad index. Try again.\n");
            continue;
        }

        let chosen = hand[index];
        match game.apply_action(pid, Action::PlayCard { card: chosen }) {
            Ok(events) => {
                if let Some(w) = trick_winner_from_events(&events) {
                    println!("✅ Trick winner: Player {}\n", w.0);
                } else {
                    println!("Events: {:?}\n", events);
                }
                return;
            }
            Err(e) => println!("Illegal move: {}. Try again.\n", e),
        }
    }
}

// ----------------------------
// Prompts
// ----------------------------

fn ask_difficulty() -> Difficulty {
    loop {
        println!("Choose bot difficulty:");
        println!("  1 = Easy   (random legal)");
        println!("  2 = Medium (cheap win else cheap)");
        println!("  3 = Hard   (partner-aware)");
        print!("diff> ");
        flush();

        match read_line_trimmed().as_str() {
            "1" => return Difficulty::Easy,
            "2" => return Difficulty::Medium,
            "3" => return Difficulty::Hard,
            _ => println!("Nope. Type 1, 2, or 3.\n"),
        }
    }
}

fn ask_hokm_suit() -> Suit {
    loop {
        println!("Choose Hokm (trump). Type: c/d/h/s");
        print!("hokm> ");
        flush();

        let s = read_line_trimmed().to_lowercase();
        match s.as_str() {
            "c" => return Suit::Clubs,
            "d" => return Suit::Diamonds,
            "h" => return Suit::Hearts,
            "s" => return Suit::Spades,
            _ => println!("Nope. Only c/d/h/s.\n"),
        }
    }
}

// ----------------------------
// Random seed helper
// ----------------------------

fn random_seed() -> u64 {
    // Grab current time in nanoseconds and smash it into u64.
    // Different run time => different seed => different shuffle.
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos() as u64
}

// ----------------------------
// Printing helpers
// ----------------------------

fn print_public_state(game: &GameState) {
    if game.current_trick.is_empty() {
        println!("Current trick: (empty)");
    } else {
        print!("Current trick: ");
        for (p, c) in &game.current_trick {
            print!("P{}:{}  ", p.0, card_to_string(*c));
        }
        println!();
    }

    println!("Turn: Player {}", game.turn.0);

    let hand0 = &game.hands[0];
    println!("Your hand (Player 0):");
    for (i, card) in hand0.iter().enumerate() {
        println!("  {:2}: {}", i, card_to_string(*card));
    }
    println!();
}

fn card_to_string(card: Card) -> String {
    let suit_char = match card.suit {
        Suit::Clubs => "C",
        Suit::Diamonds => "D",
        Suit::Hearts => "H",
        Suit::Spades => "S",
    };
    let rank_str = match card.rank as u8 {
        2..=10 => format!("{}", card.rank as u8),
        11 => "J".to_string(),
        12 => "Q".to_string(),
        13 => "K".to_string(),
        14 => "A".to_string(),
        _ => "?".to_string(),
    };
    format!("{}{}", rank_str, suit_char)
}

fn trick_winner_from_events(events: &[hokm_core::Event]) -> Option<PlayerId> {
    for e in events {
        if let hokm_core::Event::TrickEnded { winner } = e {
            return Some(*winner);
        }
    }
    None
}

// ----------------------------
// Tiny IO helpers
// ----------------------------

fn read_line_trimmed() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).expect("stdin read failed");
    s.trim().to_string()
}

fn flush() {
    io::stdout().flush().unwrap();
}
