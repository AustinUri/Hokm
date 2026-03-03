// crates/hokm_client/src/tui/app.rs
//
// Fixes added:
// ✅ Redeal loop if any player has 0 trump after Hokm is chosen (table rule).
// ✅ Bots auto-play until it's your turn (no "stuck").
// ✅ Better logs for why redeal happened.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::widgets::ListState;

use hokm_ai::Difficulty;
use hokm_core::{
    chooser_by_first_ace, deal_hands_with_dealer, sort_hand, Action, Card, GameConfig, GameState,
    Phase, PlayerId, Rank, Suit,
};

pub struct App {
    pub bot_diff: Difficulty,
    pub game: GameState,

    pub hand_state: ListState,
    pub logs: Vec<String>,

    pub chooser: PlayerId,
    pub dealer: PlayerId,
    pub hokm: Option<Suit>,

    pub base_seed: u64,

    // Used to redeal with different shuffles
    deal_attempt: u32,
}

impl App {
    pub fn new() -> Self {
        let base_seed = random_seed();
        let bot_diff = Difficulty::Easy;

        // Setup chooser/dealer from pre-draw
        let (chooser, ace) = chooser_by_first_ace(base_seed);
        let dealer = PlayerId((chooser.0 + 3) % 4);

        // Deal attempt counter
        let deal_attempt = 0;

        let config = GameConfig {
            target_round_wins: 7,
        };

        // Start with an initial deal (attempt 0)
        let hands = deal_sorted_for_ui(base_seed, dealer, deal_attempt);

        let game = GameState::new_round_with_hands_config(dealer, chooser, hands, config);

        let mut logs = vec![
            format!("Seed: {}", base_seed),
            format!(
                "Pre-draw: P{} got {} -> chooser=P{}",
                chooser.0,
                card_to_string(ace),
                chooser.0
            ),
            format!("Dealer = P{}", dealer.0),
        ];

        let mut hand_state = ListState::default();
        if !game.hands[0].is_empty() {
            hand_state.select(Some(0));
        }

        let mut app = Self {
            bot_diff,
            game,
            hand_state,
            logs,
            chooser,
            dealer,
            hokm: None,
            base_seed,
            deal_attempt,
        };

        // If chooser is not human, bot chooses hokm immediately (then redeal-check).
        if app.chooser.0 != 0 {
            app.bot_choose_hokm_and_validate();
        } else {
            app.logs
                .push("You are chooser (P0): choose Hokm.".to_string());
            app.logs
                .push("TEMP: press Enter to auto-pick Hokm from your first card suit.".to_string());
        }

        // If it is not your turn, bots should play until it becomes your turn.
        app.run_bots_until_human_turn();

        app
    }

    pub fn reset_match(&mut self) {
        *self = Self::new();
    }

    pub fn on_tick(&mut self) {}

    pub fn hand_cursor_down(&mut self) {
        let len = self.game.hands[0].len();
        if len == 0 {
            self.hand_state.select(None);
            return;
        }
        let cur = self.hand_state.selected().unwrap_or(0);
        self.hand_state.select(Some((cur + 1).min(len - 1)));
    }

    pub fn hand_cursor_up(&mut self) {
        let len = self.game.hands[0].len();
        if len == 0 {
            self.hand_state.select(None);
            return;
        }
        let cur = self.hand_state.selected().unwrap_or(0);
        self.hand_state.select(Some(cur.saturating_sub(1)));
    }

    pub fn try_play_selected_card(&mut self) {
        // If chooser is P0 and Hokm not chosen yet:
        // TEMP: Enter chooses Hokm from your first card suit.
        if self.hokm.is_none()
            && self.chooser.0 == 0
            && matches!(self.game.phase, Phase::ChoosingHokm { .. })
        {
            let Some(card) = self.game.hands[0].get(0).copied() else {
                self.logs.push("You have no cards?? (bug)".to_string());
                return;
            };

            let suit = card.suit;

            match self
                .game
                .apply_action(self.chooser, Action::ChooseHokm { suit })
            {
                Ok(_) => {
                    self.hokm = Some(suit);
                    self.logs
                        .push(format!("You chose Hokm (temporary): {:?}", suit));

                    // ✅ Apply redeal rule (any player 0 trump => redeal and choose again)
                    if !self.validate_or_redeal_after_hokm() {
                        // If redeal happened and chooser is P0, we stop here (user must choose again)
                        return;
                    }

                    // After hokm is valid, bots may need to play until your turn
                    self.run_bots_until_human_turn();
                }
                Err(e) => self.logs.push(format!("Hokm choose failed: {}", e)),
            }
            return;
        }

        // Must be your turn
        if self.game.turn.0 != 0 {
            self.logs.push("Not your turn.".to_string());
            return;
        }

        // Must be playing phase
        if !matches!(self.game.phase, Phase::Playing) {
            self.logs.push("Not in Playing phase.".to_string());
            return;
        }

        let hand = &self.game.hands[0];
        if hand.is_empty() {
            self.hand_state.select(None);
            self.logs.push("Your hand is empty.".to_string());
            return;
        }

        let idx = match self.hand_state.selected() {
            Some(i) => i.min(hand.len() - 1),
            None => {
                self.logs.push("No card selected.".to_string());
                return;
            }
        };

        let card = hand[idx];

        match self
            .game
            .apply_action(PlayerId(0), Action::PlayCard { card })
        {
            Ok(events) => {
                self.logs
                    .push(format!("You played {}", card_to_string(card)));
                for e in events {
                    self.logs.push(format!("Event: {:?}", e));
                }

                self.fix_hand_selection();
                self.run_bots_until_human_turn();
            }
            Err(e) => self.logs.push(format!("Illegal move: {}", e)),
        }
    }

    fn fix_hand_selection(&mut self) {
        let len = self.game.hands[0].len();
        if len == 0 {
            self.hand_state.select(None);
            return;
        }
        let cur = self.hand_state.selected().unwrap_or(0);
        self.hand_state.select(Some(cur.min(len - 1)));
    }

    // ------------------------------------------------------------
    // Hokm choice + redeal rule
    // ------------------------------------------------------------

    fn bot_choose_hokm_and_validate(&mut self) {
        // Bot chooses hokm from chooser hand
        let suit = choose_hokm_for_bot(&self.game.hands[self.chooser.idx()]);
        match self
            .game
            .apply_action(self.chooser, Action::ChooseHokm { suit })
        {
            Ok(_) => {
                self.hokm = Some(suit);
                self.logs
                    .push(format!("Bot chooser picked Hokm: {:?}", suit));

                // Apply redeal rule
                let _ = self.validate_or_redeal_after_hokm();
            }
            Err(e) => self
                .logs
                .push(format!("Bot Hokm choose failed (bug): {}", e)),
        }
    }

    /// Returns true if we have a valid Hokm deal and can continue playing.
    /// Returns false if we had to redeal and now we need human to choose Hokm again.
    fn validate_or_redeal_after_hokm(&mut self) -> bool {
        let Some(hokm) = self.hokm else {
            return false;
        };

        let counts = trump_count_per_player(&self.game.hands, hokm);
        let zeros: Vec<u8> = counts
            .iter()
            .enumerate()
            .filter_map(|(i, &c)| if c == 0 { Some(i as u8) } else { None })
            .collect();

        // Print counts to logs (helps debugging)
        self.logs.push(format!(
            "Trump counts {:?}: P0={} P1={} P2={} P3={}",
            hokm, counts[0], counts[1], counts[2], counts[3]
        ));

        if zeros.is_empty() {
            self.logs.push("Trump check OK ✅".to_string());
            return true;
        }

        // Someone has 0 trump -> redeal
        self.logs.push(format!(
            "Redeal ❌ (0 trump in {:?}) for players: {}",
            hokm,
            zeros
                .iter()
                .map(|p| format!("P{}", p))
                .collect::<Vec<_>>()
                .join(", ")
        ));

        // Redeal means:
        // - new hands
        // - reset core round state
        // - hokm reset
        // - chooser must pick again
        self.redeal_round();

        // If chooser is bot, it will choose again automatically and we’re good.
        if self.chooser.0 != 0 {
            self.bot_choose_hokm_and_validate();
            return true;
        }

        // Chooser is human -> user must choose again
        self.logs
            .push("You must choose Hokm again after redeal.".to_string());
        self.logs
            .push("TEMP: press Enter to auto-pick Hokm from your first card suit.".to_string());
        false
    }

    fn redeal_round(&mut self) {
        self.deal_attempt += 1;

        // fresh deal
        let hands = deal_sorted_for_ui(self.base_seed, self.dealer, self.deal_attempt);

        // reset core round state; keep config and rounds_won
        self.game.start_next_round(self.dealer, hands, self.chooser);

        // reset hokm
        self.hokm = None;

        // reset selection
        if !self.game.hands[0].is_empty() {
            self.hand_state.select(Some(0));
        } else {
            self.hand_state.select(None);
        }
    }

    // ------------------------------------------------------------
    // Bots auto-play until it's your turn
    // ------------------------------------------------------------

    fn run_bots_until_human_turn(&mut self) {
        loop {
            if matches!(
                self.game.phase,
                Phase::RoundOver { .. } | Phase::GameOver { .. }
            ) {
                self.logs.push("Round ended.".to_string());
                break;
            }

            if self.game.turn.0 == 0 {
                break;
            }

            // If we're still choosing Hokm, stop (human might need to choose)
            if matches!(self.game.phase, Phase::ChoosingHokm { .. }) {
                // If chooser is bot, it should pick. If chooser is human, stop.
                if self.chooser.0 != 0 && self.hokm.is_none() {
                    self.bot_choose_hokm_and_validate();
                    continue;
                }
                break;
            }

            if !matches!(self.game.phase, Phase::Playing) {
                break;
            }

            let pid = self.game.turn;
            let seed = self.base_seed.wrapping_add(pid.0 as u64 * 9999);

            let Some(action) = hokm_ai::choose_action(&self.game, pid, self.bot_diff, seed) else {
                self.logs
                    .push(format!("Bot P{} had no action (bug).", pid.0));
                break;
            };

            let card = match action {
                Action::PlayCard { card } => card,
                _ => {
                    self.logs
                        .push("Bot returned non-PlayCard (bug).".to_string());
                    break;
                }
            };

            match self.game.apply_action(pid, action) {
                Ok(events) => {
                    self.logs
                        .push(format!("Bot P{} played {}", pid.0, card_to_string(card)));
                    for e in events {
                        self.logs.push(format!("Event: {:?}", e));
                    }
                }
                Err(e) => {
                    self.logs.push(format!("Bot illegal move (bug): {}", e));
                    break;
                }
            }
        }

        self.fix_hand_selection();
    }
}

// ----------------------------
// Helpers
// ----------------------------

fn deal_sorted_for_ui(seed: u64, dealer: PlayerId, attempt: u32) -> [Vec<Card>; 4] {
    // Use seed + 1 + attempt to make each redeal different
    let deal_seed = seed.wrapping_add(1 + attempt as u64);
    let mut hands = deal_hands_with_dealer(deal_seed, dealer);
    for h in hands.iter_mut() {
        sort_hand(h);
    }
    hands
}

fn trump_count_per_player(hands: &[Vec<Card>; 4], hokm: Suit) -> [u8; 4] {
    let mut counts = [0u8; 4];
    for pid in 0..4usize {
        counts[pid] = hands[pid].iter().filter(|c| c.suit == hokm).count() as u8;
    }
    counts
}

fn choose_hokm_for_bot(hand: &[Card]) -> Suit {
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

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos() as u64
}
