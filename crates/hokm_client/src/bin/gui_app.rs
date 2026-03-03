// crates/hokm_client/src/bin/gui_app.rs
//
// ✅ What this file does (simple explanation):
// This file is the GUI "brain":
// 1) Draws the main menu
// 2) Draws the game table + your hand
// 3) Sends actions to hokm_core (the real rule engine)
// 4) Adds "game feel":
//    - hover inspect animation on cards
//    - fly animation when a card is played
//    - scoreboard + round-end banner
//
// IMPORTANT:
// - hokm_core decides legality + updates game state.
// - We never "hack" the rules in the UI.
// - Sorting is DISPLAY-only (does NOT change real hand order).

use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use eframe::egui;

use hokm_ai::Difficulty;
use hokm_core::{
    chooser_by_first_ace, deal_hands_with_dealer, sort_hand, Action, Card, GameConfig, GameState,
    Phase, PlayerId, Rank, Suit,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    MainMenu,
    InGame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum HandSortMode {
    Suit,
    Rank,
    Color,
}

/// This describes where players sit on the table (for animations).
#[derive(Clone, Copy, Debug)]
struct TableLayout {
    seat_center: [egui::Pos2; 4], // P0..P3 positions
    center: egui::Pos2,           // center trick position
}

/// One active flying card animation.
#[derive(Clone, Copy, Debug)]
struct ActiveFlyAnim {
    card: Card,
    from: egui::Pos2,
    to: egui::Pos2,
    start: Instant,
    duration: Duration,
}

pub struct GuiApp {
    screen: Screen,
    menu_msg: String,

    bot_diff: Difficulty,
    target_rounds: u8,

    game: GameState,
    chooser: PlayerId,
    dealer: PlayerId,
    hokm: Option<Suit>,

    // Chooser first 5 cards in deal order (used for hokm selection)
    chooser_first5: Vec<Card>,

    logs: Vec<String>,
    human: PlayerId,

    base_seed: u64,
    round_number: u32,
    deal_attempt: u32,

    pending_hokm_pick: bool,

    // Hand display sorting
    hand_sort: HandSortMode,

    // Round end banner popup
    banner_until: Option<Instant>,
    banner_text: String,
    banner_round_number: u32,

    // Animation state
    layout: Option<TableLayout>,
    fly_queue: Vec<(u8, Card)>, // (pid, card)
    fly_active: Option<ActiveFlyAnim>,
}

impl GuiApp {
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            screen: Screen::MainMenu,
            menu_msg: "Offline works. Online is locked for now.".to_string(),

            bot_diff: Difficulty::Easy,
            target_rounds: 7,

            game: dummy_game_state(),
            chooser: PlayerId(0),
            dealer: PlayerId(0),
            hokm: None,

            chooser_first5: vec![],
            logs: vec![],
            human: PlayerId(0),

            base_seed: random_seed(),
            round_number: 1,
            deal_attempt: 0,

            pending_hokm_pick: false,

            hand_sort: HandSortMode::Suit,

            banner_until: None,
            banner_text: String::new(),
            banner_round_number: 0,

            layout: None,
            fly_queue: vec![],
            fly_active: None,
        }
    }

    // ---------------- MENU / MATCH ----------------

    fn start_offline_match(&mut self) {
        self.base_seed = random_seed();
        self.round_number = 1;
        self.start_new_round();
        self.screen = Screen::InGame;
    }

    fn start_new_round(&mut self) {
        self.deal_attempt = 0;

        let seed_for_round = self
            .base_seed
            .wrapping_add(self.round_number as u64 * 1_000_000);

        let (chooser, dealer, hands, chooser_first5, mut logs) =
            setup_round(seed_for_round, self.deal_attempt);

        logs.push(format!("--- Round {} ---", self.round_number));

        let config = GameConfig {
            target_round_wins: self.target_rounds,
        };

        self.chooser = chooser;
        self.dealer = dealer;
        self.hokm = None;
        self.chooser_first5 = chooser_first5;
        self.logs = logs;

        self.game = GameState::new_round_with_hands_config(dealer, chooser, hands, config);

        self.pending_hokm_pick = self.chooser == self.human;

        // Clear animations between rounds
        self.fly_queue.clear();
        self.fly_active = None;

        self.maybe_bot_choose_hokm();
        self.run_bots_until_human_turn();
    }

    fn next_round(&mut self) {
        if matches!(self.game.phase, Phase::GameOver { .. }) {
            return;
        }
        self.round_number += 1;
        self.start_new_round();
    }

    fn reset_match(&mut self) {
        self.start_offline_match();
    }

    fn back_to_menu(&mut self) {
        self.screen = Screen::MainMenu;
        self.menu_msg = "Back to menu.".to_string();
    }

    // ---------------- HOKM CHOICE ----------------

    fn maybe_bot_choose_hokm(&mut self) {
        if self.hokm.is_some() {
            return;
        }
        if self.chooser == self.human {
            return;
        }

        // Bot chooses hokm only from first 5 cards
        let suit = choose_hokm_for_bot(&self.chooser_first5);
        self.apply_hokm_choice(suit, true);
    }

    fn apply_hokm_choice(&mut self, suit: Suit, by_bot: bool) {
        match self
            .game
            .apply_action(self.chooser, Action::ChooseHokm { suit })
        {
            Ok(_) => {
                self.hokm = Some(suit);
                self.pending_hokm_pick = false;

                self.logs.push(format!(
                    "{} chose Hokm: {:?} (based on first 5)",
                    if by_bot { "Bot" } else { "You" },
                    suit
                ));

                // Redeal rule
                if !self.trump_check_ok(suit) {
                    self.logs.push("Redeal: someone has 0 trump.".to_string());
                    self.redeal_same_round();
                    return;
                }

                self.run_bots_until_human_turn();
            }
            Err(e) => self.logs.push(format!("Hokm choose error: {}", e)),
        }
    }

    fn trump_check_ok(&mut self, hokm: Suit) -> bool {
        let mut zeros = vec![];
        for pid in 0..4u8 {
            let has_any = self.game.hands[pid as usize].iter().any(|c| c.suit == hokm);
            if !has_any {
                zeros.push(pid);
            }
        }
        if !zeros.is_empty() {
            self.logs.push(format!(
                "0 trump in {:?}: {}",
                hokm,
                zeros
                    .iter()
                    .map(|p| format!("P{}", p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
            return false;
        }
        true
    }

    fn redeal_same_round(&mut self) {
        self.deal_attempt += 1;

        let seed_for_round = self
            .base_seed
            .wrapping_add(self.round_number as u64 * 1_000_000);

        let mut hands = deal_hands_with_dealer(
            seed_for_round.wrapping_add(1 + self.deal_attempt as u64),
            self.dealer,
        );

        self.chooser_first5 = hands[self.chooser.idx()].iter().take(5).copied().collect();

        for h in hands.iter_mut() {
            sort_hand(h);
        }

        self.game.start_next_round(self.dealer, hands, self.chooser);

        self.hokm = None;
        self.pending_hokm_pick = self.chooser == self.human;

        // Clear animations because deal changed
        self.fly_queue.clear();
        self.fly_active = None;

        self.maybe_bot_choose_hokm();
    }

    // ---------------- PLAY CARDS + QUEUE ANIM ----------------

    fn play_human_card(&mut self, card: Card) {
        if self.game.turn != self.human {
            self.logs.push("Not your turn.".to_string());
            return;
        }
        if !matches!(self.game.phase, Phase::Playing) {
            self.logs.push("Not in Playing phase.".to_string());
            return;
        }

        match self
            .game
            .apply_action(self.human, Action::PlayCard { card })
        {
            Ok(events) => {
                self.queue_fly(self.human.0, card);

                self.logs.push(format!("You played {}", card_short(card)));
                for e in events {
                    self.logs.push(format!("Event: {:?}", e));
                }
                self.run_bots_until_human_turn();
            }
            Err(e) => self.logs.push(format!("Illegal move: {}", e)),
        }
    }

    fn run_bots_until_human_turn(&mut self) {
        loop {
            if matches!(
                self.game.phase,
                Phase::RoundOver { .. } | Phase::GameOver { .. }
            ) {
                break;
            }
            if self.game.turn == self.human {
                break;
            }
            if !matches!(self.game.phase, Phase::Playing) {
                break;
            }

            let pid = self.game.turn;
            let seed = self.base_seed.wrapping_add(pid.0 as u64 * 9999);

            let Some(action) = hokm_ai::choose_action(&self.game, pid, self.bot_diff, seed) else {
                self.logs.push("Bot had no action (bug)".to_string());
                break;
            };

            let card = match action {
                Action::PlayCard { card } => card,
                _ => break,
            };

            let _ = self.game.apply_action(pid, action);

            self.queue_fly(pid.0, card);

            self.logs
                .push(format!("Bot P{} played {}", pid.0, card_short(card)));
        }
    }

    // ---------------- FLY ANIMATION ENGINE ----------------

    fn queue_fly(&mut self, pid: u8, card: Card) {
        self.fly_queue.push((pid, card));
    }

    fn start_next_fly_if_needed(&mut self) {
        if self.fly_active.is_some() || self.fly_queue.is_empty() {
            return;
        }

        let Some(layout) = self.layout else {
            // layout not ready yet (very rare)
            return;
        };

        let (pid, card) = self.fly_queue.remove(0);

        self.fly_active = Some(ActiveFlyAnim {
            card,
            from: layout.seat_center[pid as usize],
            to: layout.center,
            start: Instant::now(),
            duration: Duration::from_millis(380),
        });
    }

    fn draw_fly_animation(&mut self, ui: &egui::Ui) {
        self.start_next_fly_if_needed();

        let Some(anim) = self.fly_active else {
            return;
        };

        let elapsed = Instant::now().saturating_duration_since(anim.start);
        let mut t = elapsed.as_secs_f32() / anim.duration.as_secs_f32();
        if t > 1.0 {
            t = 1.0;
        }

        // Ease-out (smooth finish)
        let eased = 1.0 - (1.0 - t) * (1.0 - t);

        let pos = anim.from.lerp(anim.to, eased);

        draw_card_at(ui, pos, anim.card);

        if t >= 1.0 {
            self.fly_active = None;
        }
    }

    // ---------------- ROUND END BANNER ----------------

    fn maybe_trigger_round_end_banner(&mut self) {
        match self.game.phase {
            Phase::RoundOver { winner, kot } => {
                if self.banner_round_number != self.round_number {
                    self.banner_round_number = self.round_number;
                    self.banner_until = Some(Instant::now() + Duration::from_millis(1600));
                    self.banner_text = format!(
                        "ROUND {} OVER\nWinner: {:?}\nKot: {}",
                        self.round_number, winner, kot
                    );
                }
            }
            Phase::GameOver { winner } => {
                if self.banner_round_number != self.round_number {
                    self.banner_round_number = self.round_number;
                    self.banner_until = Some(Instant::now() + Duration::from_millis(2200));
                    self.banner_text = format!(
                        "MATCH OVER\nWinner: {:?}\nFinal Rounds: A={}  B={}",
                        winner, self.game.rounds_won[0], self.game.rounds_won[1]
                    );
                }
            }
            _ => {}
        }
    }

    fn draw_banner_overlay(&mut self, ctx: &egui::Context) {
        let Some(until) = self.banner_until else {
            return;
        };

        if Instant::now() > until {
            self.banner_until = None;
            self.banner_text.clear();
            return;
        }

        egui::Area::new("round_end_banner".into())
            .anchor(egui::Align2::CENTER_CENTER, egui::vec2(0.0, 0.0))
            .interactable(false)
            .show(ctx, |ui| {
                let frame = egui::Frame::none()
                    .fill(egui::Color32::from_rgba_unmultiplied(0, 0, 0, 200))
                    .rounding(egui::Rounding::same(14.0))
                    .inner_margin(egui::Margin::same(18.0))
                    .stroke(egui::Stroke::new(1.0, egui::Color32::from_gray(220)));

                frame.show(ui, |ui| {
                    ui.vertical_centered(|ui| {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(self.banner_text.clone())
                                    .size(22.0)
                                    .color(egui::Color32::WHITE)
                                    .strong(),
                            )
                            .wrap(),
                        );
                    });
                });
            });
    }
}

impl eframe::App for GuiApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        match self.screen {
            Screen::MainMenu => draw_menu(self, ctx),
            Screen::InGame => draw_game(self, ctx),
        }

        if self.screen == Screen::InGame {
            self.maybe_trigger_round_end_banner();
            self.draw_banner_overlay(ctx);
        }

        ctx.request_repaint();
    }
}

// -------------------- UI --------------------

fn draw_menu(app: &mut GuiApp, ctx: &egui::Context) {
    egui::CentralPanel::default().show(ctx, |ui| {
        ui.vertical_centered(|ui| {
            ui.add_space(28.0);
            ui.heading("Hokm");
            ui.label("Offline playable. Online features are locked (in development).");
            ui.add_space(10.0);

            ui.group(|ui| ui.label(&app.menu_msg));

            ui.add_space(16.0);

            ui.group(|ui| {
                ui.label("Play");
                if ui.button("Play Offline").clicked() {
                    app.start_offline_match();
                }

                ui.add_enabled(false, egui::Button::new("Matchmaking 🔒 (in development)"));
                ui.add_enabled(
                    false,
                    egui::Button::new("Private Matches 🔒 (in development)"),
                );
                ui.add_enabled(false, egui::Button::new("Tournaments 🔒 (in development)"));
            });
        });
    });
}

fn draw_game(app: &mut GuiApp, ctx: &egui::Context) {
    egui::TopBottomPanel::top("top").show(ctx, |ui| {
        ui.horizontal(|ui| {
            if ui.button("← Menu").clicked() {
                app.back_to_menu();
            }

            ui.separator();
            ui.label(format!("Round {}", app.round_number));

            ui.separator();
            ui.label(format!(
                "Rounds  A:{}  B:{}",
                app.game.rounds_won[0], app.game.rounds_won[1]
            ));
            ui.label(format!(
                "Tricks  A:{}  B:{}",
                app.game.tricks_taken[0], app.game.tricks_taken[1]
            ));

            ui.separator();
            ui.label(format!("Turn P{}", app.game.turn.0));
            ui.label(format!("Phase: {:?}", app.game.phase));

            ui.separator();
            if ui.button("Reset Match").clicked() {
                app.reset_match();
            }

            if matches!(app.game.phase, Phase::RoundOver { .. })
                && ui.button("Next Round").clicked()
            {
                app.next_round();
            }
        });
    });

    egui::CentralPanel::default().show(ctx, |ui| {
        ui.columns(2, |cols| {
            cols[0].heading("Table");

            let human_is_choosing_hokm =
                app.pending_hokm_pick && app.hokm.is_none() && app.chooser == app.human;

            if human_is_choosing_hokm {
                cols[0].group(|ui| {
                    ui.label("Choose Hokm (trump):");
                    ui.horizontal(|ui| {
                        for (label, suit) in [
                            ("Clubs", Suit::Clubs),
                            ("Diamonds", Suit::Diamonds),
                            ("Hearts", Suit::Hearts),
                            ("Spades", Suit::Spades),
                        ] {
                            if ui.button(label).clicked() {
                                app.apply_hokm_choice(suit, false);
                            }
                        }
                    });
                });
                cols[0].add_space(6.0);
            }

            // Table area rect
            let arena_h = 380.0;
            let (arena_rect, _) = cols[0].allocate_exact_size(
                egui::vec2(cols[0].available_width(), arena_h),
                egui::Sense::hover(),
            );

            // Draw table + get layout positions for animations
            let layout = draw_table_arena_swapped(&mut cols[0], arena_rect, &app.game);
            app.layout = Some(layout);

            cols[0].add_space(8.0);

            cols[0].horizontal(|ui| {
                ui.label("Sort:");
                if ui
                    .selectable_label(app.hand_sort == HandSortMode::Suit, "Suit")
                    .clicked()
                {
                    app.hand_sort = HandSortMode::Suit;
                }
                if ui
                    .selectable_label(app.hand_sort == HandSortMode::Rank, "Rank")
                    .clicked()
                {
                    app.hand_sort = HandSortMode::Rank;
                }
                if ui
                    .selectable_label(app.hand_sort == HandSortMode::Color, "Color")
                    .clicked()
                {
                    app.hand_sort = HandSortMode::Color;
                }
            });

            cols[0].label("Your hand:");

            egui::ScrollArea::horizontal()
                .id_salt("hand_scroll_unique")
                .max_height(150.0)
                .show(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        if human_is_choosing_hokm {
                            for card in app.chooser_first5.clone() {
                                let _ = card_widget(ui, card, false);
                            }
                            let hidden = 13usize.saturating_sub(app.chooser_first5.len());
                            for _ in 0..hidden {
                                let _ = card_back_widget(ui);
                            }
                        } else {
                            let enabled = app.game.turn == app.human
                                && matches!(app.game.phase, Phase::Playing);

                            let mut cards: Vec<Card> = app.game.hands[app.human.idx()].clone();
                            sort_for_display(&mut cards, app.hand_sort);

                            for card in cards {
                                if card_widget(ui, card, enabled).clicked() {
                                    app.play_human_card(card);
                                }
                            }
                        }
                    });
                });

            cols[1].heading("Logs");
            egui::ScrollArea::vertical()
                .id_salt("logs_scroll_unique")
                .show(&mut cols[1], |ui| {
                    let start = app.logs.len().saturating_sub(220);
                    for line in &app.logs[start..] {
                        ui.label(line);
                    }
                });

            // ✅ BUG FIX:
            // cols[0] IS the Ui. There is no cols[0].ui field.
            app.draw_fly_animation(&cols[0]);
        });
    });
}

// -------------------- Sorting (display only) --------------------

fn sort_for_display(cards: &mut Vec<Card>, mode: HandSortMode) {
    match mode {
        HandSortMode::Suit => {
            cards.sort_by_key(|c| (suit_order(c.suit), std::cmp::Reverse(rank_value(c.rank))));
        }
        HandSortMode::Rank => {
            cards.sort_by_key(|c| (std::cmp::Reverse(rank_value(c.rank)), suit_order(c.suit)));
        }
        HandSortMode::Color => {
            cards.sort_by_key(|c| {
                (
                    color_group(c.suit),
                    suit_order(c.suit),
                    std::cmp::Reverse(rank_value(c.rank)),
                )
            });
        }
    }
}

fn suit_order(s: Suit) -> u8 {
    match s {
        Suit::Clubs => 0,
        Suit::Diamonds => 1,
        Suit::Hearts => 2,
        Suit::Spades => 3,
    }
}

fn color_group(s: Suit) -> u8 {
    match s {
        Suit::Clubs | Suit::Spades => 0,
        Suit::Diamonds | Suit::Hearts => 1,
    }
}

fn rank_value(r: Rank) -> u8 {
    match r {
        Rank::Two => 2,
        Rank::Three => 3,
        Rank::Four => 4,
        Rank::Five => 5,
        Rank::Six => 6,
        Rank::Seven => 7,
        Rank::Eight => 8,
        Rank::Nine => 9,
        Rank::Ten => 10,
        Rank::Jack => 11,
        Rank::Queen => 12,
        Rank::King => 13,
        Rank::Ace => 14,
    }
}

// -------------------- Table drawing --------------------

fn draw_table_arena_swapped(ui: &mut egui::Ui, rect: egui::Rect, game: &GameState) -> TableLayout {
    let painter = ui.painter();

    painter.rect_filled(rect, 14.0, egui::Color32::from_rgb(18, 75, 48));

    let center = rect.center();
    let slot_w = rect.width() * 0.22;
    let slot_h = rect.height() * 0.18;

    let top = egui::Rect::from_center_size(
        egui::pos2(center.x, rect.top() + slot_h * 0.75),
        egui::vec2(slot_w * 1.6, slot_h),
    );
    let bottom = egui::Rect::from_center_size(
        egui::pos2(center.x, rect.bottom() - slot_h * 0.75),
        egui::vec2(slot_w * 1.6, slot_h),
    );
    let left = egui::Rect::from_center_size(
        egui::pos2(rect.left() + slot_w * 0.65, center.y),
        egui::vec2(slot_w * 1.2, slot_h),
    );
    let right = egui::Rect::from_center_size(
        egui::pos2(rect.right() - slot_w * 0.65, center.y),
        egui::vec2(slot_w * 1.2, slot_h),
    );
    let mid = egui::Rect::from_center_size(center, egui::vec2(slot_w * 1.9, slot_h * 1.7));

    // Seats:
    // top=P2 partner, left=P1, right=P3, bottom=P0 you
    draw_player_slot(
        painter,
        top,
        "P2 (partner)",
        game.hands[2].len(),
        card_in_trick(game, 2),
    );
    draw_player_slot(
        painter,
        left,
        "P1",
        game.hands[1].len(),
        card_in_trick(game, 1),
    );
    draw_player_slot(
        painter,
        right,
        "P3",
        game.hands[3].len(),
        card_in_trick(game, 3),
    );
    draw_player_slot(
        painter,
        bottom,
        "YOU (P0)",
        game.hands[0].len(),
        card_in_trick(game, 0),
    );

    painter.rect_filled(mid, 12.0, egui::Color32::from_rgb(10, 45, 28));
    painter.rect_stroke(mid, 12.0, (1.0, egui::Color32::from_gray(210)));

    let mut lines = vec!["Current Trick".to_string()];
    if game.current_trick.is_empty() {
        lines.push("(empty)".to_string());
    } else {
        for (p, c) in &game.current_trick {
            lines.push(format!("P{}: {}", p.0, card_short(*c)));
        }
    }

    painter.text(
        mid.center(),
        egui::Align2::CENTER_CENTER,
        lines.join("\n"),
        egui::FontId::proportional(14.0),
        egui::Color32::WHITE,
    );

    // Build seat centers for animations
    let seat_center = [
        bottom.center(), // P0
        left.center(),   // P1
        top.center(),    // P2
        right.center(),  // P3
    ];

    TableLayout {
        seat_center,
        center: mid.center(),
    }
}

fn draw_player_slot(
    painter: &egui::Painter,
    rect: egui::Rect,
    name: &str,
    hand_count: usize,
    card: Option<Card>,
) {
    painter.rect_filled(rect, 12.0, egui::Color32::from_rgb(14, 60, 38));
    painter.rect_stroke(rect, 12.0, (1.0, egui::Color32::from_gray(220)));

    painter.text(
        egui::pos2(rect.center().x, rect.top() + 6.0),
        egui::Align2::CENTER_TOP,
        format!("{}  [{}]", name, hand_count),
        egui::FontId::proportional(13.0),
        egui::Color32::WHITE,
    );

    let played = match card {
        Some(c) => format!("Played: {}", card_short(c)),
        None => "Played: —".to_string(),
    };

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        played,
        egui::FontId::proportional(12.0),
        egui::Color32::WHITE,
    );
}

fn card_in_trick(game: &GameState, pid: u8) -> Option<Card> {
    for (p, c) in &game.current_trick {
        if p.0 == pid {
            return Some(*c);
        }
    }
    None
}

// -------------------- Card drawing --------------------

fn card_widget(ui: &mut egui::Ui, card: Card, enabled: bool) -> egui::Response {
    let base_size = egui::vec2(74.0, 112.0);
    let (base_rect, resp) = ui.allocate_exact_size(base_size, egui::Sense::click());
    let painter = ui.painter();

    // time is used for tiny wave on hover
    let t = ui.input(|i| i.time) as f32;
    let hovered = resp.hovered();

    let wave = ((t * 6.0).sin() * 0.5 + 0.5) as f32; // 0..1
    let lift_px = if hovered { 8.0 + 2.0 * wave } else { 0.0 };
    let grow_px = if hovered { 4.0 + 2.0 * wave } else { 0.0 };

    let mut draw_rect = base_rect.translate(egui::vec2(0.0, -lift_px));
    draw_rect = draw_rect.expand(grow_px);

    draw_card_rect(painter, draw_rect, card, enabled, hovered);

    if !enabled {
        resp.on_disabled_hover_text("Wait for your turn")
    } else {
        resp
    }
}

fn draw_card_at(ui: &egui::Ui, center: egui::Pos2, card: Card) {
    let painter = ui.painter();
    let size = egui::vec2(80.0, 122.0);
    let rect = egui::Rect::from_center_size(center, size);
    draw_card_rect(painter, rect, card, true, false);
}

fn draw_card_rect(
    painter: &egui::Painter,
    rect: egui::Rect,
    card: Card,
    enabled: bool,
    hovered: bool,
) {
    let bg = if enabled {
        egui::Color32::from_rgb(245, 245, 245)
    } else {
        egui::Color32::from_rgb(190, 190, 190)
    };

    painter.rect_filled(rect, 12.0, bg);
    painter.rect_stroke(rect, 12.0, (1.0, egui::Color32::from_gray(60)));

    let is_red = matches!(card.suit, Suit::Hearts | Suit::Diamonds);
    let text_color = if is_red {
        egui::Color32::from_rgb(170, 30, 30)
    } else {
        egui::Color32::BLACK
    };

    let text = card_short(card);

    painter.text(
        rect.left_top() + egui::vec2(8.0, 8.0),
        egui::Align2::LEFT_TOP,
        text.clone(),
        egui::FontId::proportional(16.0),
        text_color,
    );

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::proportional(22.0),
        text_color,
    );

    if hovered {
        painter.rect_stroke(rect, 12.0, (2.0, egui::Color32::from_rgb(40, 140, 240)));
    }
}

fn card_back_widget(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(egui::vec2(74.0, 112.0), egui::Sense::hover());
    let painter = ui.painter();

    painter.rect_filled(rect, 12.0, egui::Color32::from_rgb(30, 60, 120));
    painter.rect_stroke(rect, 12.0, (1.0, egui::Color32::from_gray(40)));

    painter.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        "🂠",
        egui::FontId::proportional(26.0),
        egui::Color32::from_rgb(240, 240, 240),
    );

    resp
}

// -------------------- Setup / helpers --------------------

fn setup_round(
    seed: u64,
    deal_attempt: u32,
) -> (PlayerId, PlayerId, [Vec<Card>; 4], Vec<Card>, Vec<String>) {
    let mut logs = vec![];

    let (chooser, ace) = chooser_by_first_ace(seed);
    let dealer = PlayerId((chooser.0 + 3) % 4);

    logs.push(format!(
        "Pre-draw: P{} got {} -> chooser=P{}",
        chooser.0,
        card_short(ace),
        chooser.0
    ));
    logs.push(format!("Dealer=P{}", dealer.0));

    let mut hands = deal_hands_with_dealer(seed.wrapping_add(1 + deal_attempt as u64), dealer);

    let chooser_first5: Vec<Card> = hands[chooser.idx()].iter().take(5).copied().collect();

    for h in hands.iter_mut() {
        sort_hand(h);
    }

    (chooser, dealer, hands, chooser_first5, logs)
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

fn card_short(card: Card) -> String {
    let suit = match card.suit {
        Suit::Clubs => "♣",
        Suit::Diamonds => "♦",
        Suit::Hearts => "♥",
        Suit::Spades => "♠",
    };

    let rank = match card.rank as u8 {
        2..=10 => format!("{}", card.rank as u8),
        11 => "J".to_string(),
        12 => "Q".to_string(),
        13 => "K".to_string(),
        14 => "A".to_string(),
        _ => "?".to_string(),
    };

    format!("{}{}", rank, suit)
}

fn dummy_game_state() -> GameState {
    let config = GameConfig {
        target_round_wins: 7,
    };
    GameState::new_round_with_hands_config(
        PlayerId(0),
        PlayerId(0),
        [vec![], vec![], vec![], vec![]],
        config,
    )
}

fn random_seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos() as u64
}
