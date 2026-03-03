// Hokm/crates/hokm_client/src/main.rs
//
// ✅ WHAT THIS PROGRAM DOES (VERY SIMPLE):
//
// You run it.
// 1) You choose bot difficulty (easy / medium / hard).
// 2) You choose how many rounds win the match (classic = 7).
// 3) Every round, we do your Hokm setup rules:
//
//    A) PRE-DRAW:
//       We "draw" cards one by one until the first Ace shows up.
//       The player who gets the first Ace = CHOOSER (ONLY).
//
//    B) DEALER:
//       Dealer is the player BEFORE the chooser.
//       Example: chooser=3 => dealer=2
//
//    C) REAL DEAL:
//       After chooser/dealer are known, we deal again for real.
//       Dealing starts from LEFT of dealer (dealer.next()).
//
//    D) CHOOSER PICKS HOKM (TRUMP):
//       Option A: only Player 0 is human.
//       - If chooser == Player 0: you choose hokm (type c/d/h/s).
//       - Else: bot chooses hokm automatically.
//
//    E) IMPORTANT TABLE RULE YOU ADDED:
//       After hokm is chosen, if ANY player has 0 trump cards,
//       we REDEAL the whole round and chooser chooses again.
//       This repeats until everyone has at least 1 trump card.
//
//    F) PLAY THE ROUND:
//       Round ends when a team reaches 7 tricks.
//       Kot = loser has 0 tricks (core handles it).
//
// 4) Repeat rounds until a team reaches target rounds -> match over.
//
// ✅ IMPROVEMENT ADDED NOW:
// When we do the "0 trump" check, we print:
// - how many trump cards each player has
// - which players have ZERO trump
//
// This helps you see exactly why a redeal happened.

use std::io::{self, Write};
use std::time::{SystemTime, UNIX_EPOCH};

use hokm_ai::Difficulty;
use hokm_core::{
    chooser_by_first_ace, deal_hands_with_dealer, sort_hand, Action, Card, GameConfig, GameState,
    Phase, PlayerId, Rank, Suit,
};

fn main() {
    println!("hokm_client: started ✅");

    // -------------------------------------------------
    // 1) Choose bot difficulty ONCE for the whole match
    // -------------------------------------------------
    let bot_diff = ask_difficulty();
    println!("Bot difficulty set to: {:?}\n", bot_diff);

    // -------------------------------------------------
    // 2) Choose how many rounds win the match
    // -------------------------------------------------
    let target_rounds = ask_target_rounds();
    println!("Match length: first to {} rounds wins.\n", target_rounds);

    // -------------------------------------------------
    // 3) Random base seed for the whole match
    // -------------------------------------------------
    // This makes each program run produce different games.
    // We derive round seeds from it so each round is different too.
    let base_seed: u64 = random_seed();
    println!("Base seed this match: {}\n", base_seed);

    // The core GameState has a config for "target rounds to win"
    let config = GameConfig {
        target_round_wins: target_rounds,
    };

    println!("\n==============================");
    println!("MATCH START");
    println!("Target rounds to win: {}", target_rounds);
    println!("==============================\n");

    // -------------------------------------------------
    // Round 1: create GameState from scratch (because it needs config)
    // -------------------------------------------------
    let mut round_number: u32 = 1;

    // Build round data using your full setup rules (including redeal rule)
    let (dealer, chooser, hands, hokm, round_seed) = setup_round_data(base_seed, round_number);

    // Create GameState for the match using round 1 data + config
    let mut game = GameState::new_round_with_hands_config(dealer, chooser, hands, config);

    // Apply hokm choice (chooser chooses!)
    game.apply_action(chooser, Action::ChooseHokm { suit: hokm })
        .expect("chooser should be able to choose hokm");

    // Play round 1
    play_one_round(&mut game, bot_diff, round_seed, chooser, dealer, hokm);

    // -------------------------------------------------
    // Next rounds until match ends (GameOver)
    // -------------------------------------------------
    while !matches!(game.phase, Phase::GameOver { .. }) {
        round_number += 1;

        println!("\n==============================");
        println!("NEXT ROUND {}", round_number);
        println!(
            "Scoreboard (rounds): Team A = {}, Team B = {}",
            game.rounds_won[0], game.rounds_won[1]
        );
        println!("==============================\n");

        // Build next round data (including redeal rule)
        let (new_dealer, new_chooser, new_hands, new_hokm, new_round_seed) =
            setup_round_data(base_seed, round_number);

        // Reset round state BUT keep match score + config
        game.start_next_round(new_dealer, new_hands, new_chooser);

        // Apply hokm
        game.apply_action(new_chooser, Action::ChooseHokm { suit: new_hokm })
            .expect("chooser should be able to choose hokm");

        // Play the round
        play_one_round(
            &mut game,
            bot_diff,
            new_round_seed,
            new_chooser,
            new_dealer,
            new_hokm,
        );
    }

    // Match ended
    if let Phase::GameOver { winner } = game.phase {
        println!("\n🏆 MATCH OVER 🏆");
        println!("Winner team: {:?}", winner);
        println!(
            "Final rounds: Team A = {}, Team B = {}",
            game.rounds_won[0], game.rounds_won[1]
        );
    }
}

// =====================================================
// ROUND SETUP (YOUR RULES + REDEAL RULE + DEBUG OUTPUT)
// =====================================================

/// Setup one round using your exact Hokm rules.
///
/// Returns:
/// (dealer, chooser, hands_sorted, hokm, round_seed)
///
/// NOTE:
/// - Hands returned are ALREADY SORTED for nicer printing.
/// - But we also print the first 5 cards for chooser BEFORE sorting.
fn setup_round_data(
    base_seed: u64,
    round_number: u32,
) -> (PlayerId, PlayerId, [Vec<Card>; 4], Suit, u64) {
    // Each round uses its own derived seed so it’s different.
    let round_seed = base_seed.wrapping_add(round_number as u64 * 1_000_000);

    // -----------------------------
    // A) PRE-DRAW: first Ace => chooser
    // -----------------------------
    let pre_seed = round_seed;
    let (chooser, ace_card) = chooser_by_first_ace(pre_seed);

    println!(
        "Pre-draw: Player {} got {} -> chooser = Player {}",
        chooser.0,
        card_to_string(ace_card),
        chooser.0
    );

    // -----------------------------
    // B) Dealer is player BEFORE chooser
    // -----------------------------
    let dealer = PlayerId((chooser.0 + 3) % 4);
    println!("Dealer = Player {} (before chooser)\n", dealer.0);

    // -----------------------------
    // C + D + E) Deal + choose hokm + check trump count
    // -----------------------------
    // We might have to deal multiple times if someone has 0 trump cards.
    // Each attempt uses a different deal seed so the cards change.
    let mut attempt: u32 = 0;

    loop {
        attempt += 1;

        // Deal seed changes each attempt
        let deal_seed = round_seed.wrapping_add(1 + attempt as u64);

        // Deal in deal-order starting left of dealer
        let mut hands = deal_hands_with_dealer(deal_seed, dealer);

        // Show chooser’s first 5 cards BEFORE sorting (real deal order)
        println!(
            "First 5 cards chooser Player {} got (deal order):",
            chooser.0
        );
        for i in 0..5 {
            let c = hands[chooser.idx()][i];
            println!("  {}", card_to_string(c));
        }
        println!();

        // Sort hands so the printed hand looks nice
        for h in hands.iter_mut() {
            sort_hand(h);
        }

        // Chooser chooses hokm (Option A: only player 0 is human)
        let hokm = choose_hokm_with_option_a(chooser, &hands[chooser.idx()]);

        // ✅ IMPROVEMENT:
        // Count how many trump cards each player has.
        let trump_counts = trump_count_per_player(&hands, hokm);

        // Print the counts so you SEE what happened.
        println!("Trump counts for hokm {:?}:", hokm);
        println!(
            "  P0: {}   P1: {}   P2: {}   P3: {}",
            trump_counts[0], trump_counts[1], trump_counts[2], trump_counts[3]
        );

        // Find who has 0 trump
        let zero_players = players_with_zero_trump_counts(&trump_counts);

        if zero_players.is_empty() {
            // Everyone has at least 1 trump card: round is valid
            println!("✅ Trump check passed (no one has 0 {:?})\n", hokm);
            return (dealer, chooser, hands, hokm, round_seed);
        } else {
            // Someone has 0 trump: redeal
            print!("❌ Redeal: these players have 0 {:?}: ", hokm);
            for (i, pid) in zero_players.iter().enumerate() {
                if i > 0 {
                    print!(", ");
                }
                print!("P{}", pid.0);
            }
            println!();
            println!("Dealing again...\n");
        }
    }
}

/// Option A:
/// - Only Player 0 is human.
/// - If chooser != Player 0, bot chooses hokm automatically.
fn choose_hokm_with_option_a(chooser: PlayerId, chooser_hand: &[Card]) -> Suit {
    if chooser.0 == 0 {
        println!("You are the chooser (Player 0). Choose hokm now.");
        ask_hokm_suit()
    } else {
        let suit = choose_hokm_for_bot(chooser_hand);
        println!(
            "Chooser is Player {} (bot). Bot chooses hokm: {:?}\n",
            chooser.0, suit
        );
        suit
    }
}

/// Count how many cards of hokm suit each player has.
/// Returns [count_p0, count_p1, count_p2, count_p3]
fn trump_count_per_player(hands: &[Vec<Card>; 4], hokm: Suit) -> [u8; 4] {
    let mut counts = [0u8; 4];
    for pid in 0..4usize {
        let c = hands[pid].iter().filter(|card| card.suit == hokm).count();
        counts[pid] = c as u8;
    }
    counts
}

/// From counts, return list of players with 0 trump
fn players_with_zero_trump_counts(counts: &[u8; 4]) -> Vec<PlayerId> {
    let mut out = Vec::new();
    for pid in 0..4u8 {
        if counts[pid as usize] == 0 {
            out.push(PlayerId(pid));
        }
    }
    out
}

// =====================================================
// PLAY ONE FULL ROUND (TO 7 TRICKS)
// =====================================================

fn play_one_round(
    game: &mut GameState,
    bot_diff: Difficulty,
    round_seed: u64,
    chooser: PlayerId,
    dealer: PlayerId,
    hokm: Suit,
) {
    println!("--- ROUND START ---");
    println!("Dealer:  Player {}", dealer.0);
    println!("Chooser: Player {}", chooser.0);
    println!("Hokm:    {:?}\n", hokm);

    // Trick counter just for printing
    let mut trick_number: u32 = 1;

    // Keep playing tricks until round ends (or match ends)
    loop {
        if matches!(game.phase, Phase::RoundOver { .. } | Phase::GameOver { .. }) {
            break;
        }

        println!("\n=== Trick {} ===", trick_number);

        // Change seed each trick so "easy random" bot doesn't always pick same
        let trick_seed = round_seed.wrapping_add(trick_number as u64 * 9999);

        play_one_trick(game, bot_diff, trick_seed);

        trick_number += 1;

        println!(
            "Tricks taken: Team A = {}, Team B = {}",
            game.tricks_taken[0], game.tricks_taken[1]
        );
    }

    // Print round summary
    match game.phase {
        Phase::RoundOver { winner, kot } => {
            println!("\n🏁 ROUND OVER");
            println!("Winner team: {:?} (A=0&2, B=1&3)", winner);
            println!("Kot: {}", kot);
            println!(
                "Round tricks: Team A = {}, Team B = {}",
                game.tricks_taken[0], game.tricks_taken[1]
            );
            println!(
                "Scoreboard (rounds): Team A = {}, Team B = {}",
                game.rounds_won[0], game.rounds_won[1]
            );
        }
        Phase::GameOver { winner } => {
            println!("\n🏆 MATCH-ENDING ROUND FINISHED");
            println!("Winner team: {:?}", winner);
            println!(
                "Final scoreboard (rounds): Team A = {}, Team B = {}",
                game.rounds_won[0], game.rounds_won[1]
            );
        }
        _ => {}
    }
}

fn play_one_trick(game: &mut GameState, bot_diff: Difficulty, seed: u64) {
    // Trick ends when 4 cards played and core clears current_trick.
    let mut trick_started = false;

    loop {
        // If trick already started and now it is empty => trick ended
        if trick_started && game.current_trick.is_empty() {
            return;
        }

        // If there's any card in the trick, it started
        if !game.current_trick.is_empty() {
            trick_started = true;
        }

        // Print what you can see
        print_public_state(game);

        let pid = game.turn;

        // Only player 0 is human
        if pid.0 == 0 {
            play_human_card(game, pid);
        } else {
            play_bot(game, pid, bot_diff, seed.wrapping_add(pid.0 as u64 * 1234));
        }
    }
}

// =====================================================
// BOT + HUMAN PLAY
// =====================================================

fn play_bot(game: &mut GameState, pid: PlayerId, diff: Difficulty, seed: u64) {
    // Ask bot to choose an action
    let action = hokm_ai::choose_action(game, pid, diff, seed).expect("bot had no action");

    // Bot should always return PlayCard during Playing
    let hokm_core::Action::PlayCard { card } = action else {
        panic!("bot returned non-PlayCard (bug)");
    };

    println!("Bot P{} plays {}", pid.0, card_to_string(card));

    // Apply bot move
    let events = game
        .apply_action(pid, action)
        .expect("bot played illegal move (bug)");

    // If trick ended, show winner quickly
    if let Some(w) = trick_winner_from_events(&events) {
        println!("✅ Trick winner: Player {}\n", w.0);
    }
}

fn play_human_card(game: &mut GameState, pid: PlayerId) {
    loop {
        // Show legal cards (helps you not fight the follow-suit rule)
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

        // Must be a number
        let Ok(index) = input.parse::<usize>() else {
            println!("Type a number index (0,1,2...).\n");
            continue;
        };

        // Must exist in your hand
        let hand = &game.hands[pid.idx()];
        if index >= hand.len() {
            println!("Bad index. Try again.\n");
            continue;
        }

        let chosen = hand[index];

        // Apply move. If illegal, core rejects it.
        match game.apply_action(pid, Action::PlayCard { card: chosen }) {
            Ok(events) => {
                if let Some(w) = trick_winner_from_events(&events) {
                    println!("✅ Trick winner: Player {}\n", w.0);
                }
                return;
            }
            Err(e) => println!("Illegal move: {}. Try again.\n", e),
        }
    }
}

// =====================================================
// BOT HOKM CHOICE (SIMPLE)
// =====================================================

fn choose_hokm_for_bot(hand: &[Card]) -> Suit {
    // Simple heuristic:
    // - pick suit with most cards
    // - tie-break by high-card score (A,K,Q,J,10)

    fn high_score(rank: Rank) -> u16 {
        match rank {
            Rank::Ace => 5,
            Rank::King => 4,
            Rank::Queen => 3,
            Rank::Jack => 2,
            Rank::Ten => 1,
            _ => 0,
        }
    }

    let suits = [Suit::Clubs, Suit::Diamonds, Suit::Hearts, Suit::Spades];

    let mut best_suit = Suit::Clubs;
    let mut best_count: u8 = 0;
    let mut best_high: u16 = 0;

    for s in suits {
        let mut count: u8 = 0;
        let mut high: u16 = 0;

        for c in hand {
            if c.suit == s {
                count += 1;
                high += high_score(c.rank);
            }
        }

        if (count > best_count) || (count == best_count && high > best_high) {
            best_suit = s;
            best_count = count;
            best_high = high;
        }
    }

    best_suit
}

// =====================================================
// PROMPTS
// =====================================================

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

fn ask_target_rounds() -> u8 {
    loop {
        println!("How many rounds to win the match? (classic is 7)");
        print!("target_rounds> ");
        flush();

        let s = read_line_trimmed();
        let Ok(v) = s.parse::<u8>() else {
            println!("Type a number like 1, 3, 5, 7.\n");
            continue;
        };

        if v == 0 {
            println!("0 is nonsense. Choose at least 1.\n");
            continue;
        }

        if v > 13 {
            println!("Too big for now. Choose 1..13.\n");
            continue;
        }

        return v; // ✅ THIS IS ALL YOU NEED
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

// =====================================================
// RANDOM SEED
// =====================================================

fn random_seed() -> u64 {
    // Time in nanoseconds -> good enough for random-ish seed
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos() as u64
}

// =====================================================
// PRINTING HELPERS
// =====================================================

fn print_public_state(game: &GameState) {
    // Show trick cards (public info)
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

    // Show ONLY the human hand (player 0)
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

// =====================================================
// TINY IO HELPERS
// =====================================================

fn read_line_trimmed() -> String {
    let mut s = String::new();
    io::stdin().read_line(&mut s).expect("stdin read failed");
    s.trim().to_string()
}

fn flush() {
    io::stdout().flush().unwrap();
}
