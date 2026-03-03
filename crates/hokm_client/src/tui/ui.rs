// crates/hokm_client/src/tui/ui.rs
//
// Table layout:
//
// ┌──────────── Status ────────────┐
// │ chooser/dealer/hokm/turn/phase  │
// └─────────────────────────────────┘
//
// ┌─────────────── Table (center) ───────────────────┐
// │                    [ P1 ]                         │
// │                     card                          │
// │      [ P2 ]      ┌────────┐      [ P3 ]          │
// │       card       │ trick  │       card           │
// │                  └────────┘                      │
// │                    [ P0 ]                         │
// └───────────────────────────────────────────────────┘
//
// ┌──────── Your Hand ────────┐ ┌──────── Logs ──────┐
// │ selection highlight        │ │ last messages      │
// └────────────────────────────┘ └────────────────────┘

use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, Paragraph, Wrap},
    Frame,
};

use crate::tui::app::App;

pub fn draw(f: &mut Frame, app: &mut App) {
    // Main vertical split: status / table / bottom panels
    let root = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(6),  // status
            Constraint::Min(10),    // table
            Constraint::Length(12), // hand + logs
        ])
        .split(f.area());

    draw_status(f, app, root[0]);
    draw_table(f, app, root[1]);
    draw_bottom(f, app, root[2]);
}

fn draw_status(f: &mut Frame, app: &App, area: Rect) {
    let hokm_text = match app.hokm {
        Some(s) => format!("{:?}", s),
        None => "NOT CHOSEN".to_string(),
    };

    let lines = vec![
        Line::from(vec![
            Span::raw("Chooser: "),
            Span::styled(
                format!("P{}", app.chooser.0),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("   Dealer: "),
            Span::styled(
                format!("P{}", app.dealer.0),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("   Hokm: "),
            Span::styled(hokm_text, Style::default().add_modifier(Modifier::BOLD)),
        ]),
        Line::from(vec![
            Span::raw("Turn: "),
            Span::styled(
                format!("P{}", app.game.turn.0),
                Style::default().add_modifier(Modifier::BOLD),
            ),
            Span::raw("   Phase: "),
            Span::raw(format!("{:?}", app.game.phase)),
        ]),
        Line::from(vec![
            Span::raw("Rounds (A/B): "),
            Span::raw(format!(
                "{} / {}",
                app.game.rounds_won[0], app.game.rounds_won[1]
            )),
            Span::raw("   Tricks (A/B): "),
            Span::raw(format!(
                "{} / {}",
                app.game.tricks_taken[0], app.game.tricks_taken[1]
            )),
        ]),
        Line::from("Keys: j/k or ↑/↓ move | Enter play | r restart | q quit"),
    ];

    let p = Paragraph::new(lines)
        .block(Block::default().title("Status").borders(Borders::ALL))
        .wrap(Wrap { trim: true });

    f.render_widget(p, area);
}

fn draw_table(f: &mut Frame, app: &App, area: Rect) {
    // Draw a big outer block first
    let outer = Block::default().title("Table").borders(Borders::ALL);
    f.render_widget(outer, area);

    // Inner area where we place the table widgets
    let inner = inset(area, 1);

    // Split inner into 3 rows: top player, middle row, bottom player
    let rows = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(4),
            Constraint::Length(3),
        ])
        .split(inner);

    // Top row = P1 panel (centered)
    draw_player_panel(
        f,
        app,
        rows[0],
        1,
        Alignment::Center,
        trick_card_for(app, 1),
    );

    // Middle row split into 3 columns: left=P2, center=Trick, right=P3
    let mid = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(30),
            Constraint::Percentage(40),
            Constraint::Percentage(30),
        ])
        .split(rows[1]);

    draw_player_panel(f, app, mid[0], 2, Alignment::Center, trick_card_for(app, 2));

    draw_trick_center(f, app, mid[1]);

    draw_player_panel(f, app, mid[2], 3, Alignment::Center, trick_card_for(app, 3));

    // Bottom row = P0 panel (centered)
    draw_player_panel(
        f,
        app,
        rows[2],
        0,
        Alignment::Center,
        trick_card_for(app, 0),
    );
}

fn draw_trick_center(f: &mut Frame, app: &App, area: Rect) {
    // Center box showing current trick in a compact way
    let mut lines = Vec::new();

    if app.game.current_trick.is_empty() {
        lines.push(Line::from("Trick: (empty)"));
    } else {
        lines.push(Line::from("Trick:"));
        // Show in play order
        for (p, c) in &app.game.current_trick {
            lines.push(Line::from(format!("P{}: {}", p.0, card_short(*c))));
        }
    }

    let p = Paragraph::new(lines)
        .block(
            Block::default()
                .title("Current Trick")
                .borders(Borders::ALL),
        )
        .alignment(Alignment::Center)
        .wrap(Wrap { trim: true });

    f.render_widget(p, area);
}

fn draw_player_panel(
    f: &mut Frame,
    app: &App,
    area: Rect,
    pid: u8,
    align: Alignment,
    trick_card: Option<String>,
) {
    let hand_count = app.game.hands[pid as usize].len();

    // Show name + card count, and if they played a card in this trick, show it.
    let title = format!("P{} ({})", pid, hand_count);

    let card_line = match trick_card {
        Some(s) => format!("Played: {}", s),
        None => "Played: -".to_string(),
    };

    let turn_marker = if app.game.turn.0 == pid {
        " <- turn"
    } else {
        ""
    };
    let chooser_marker = if app.chooser.0 == pid {
        " (chooser)"
    } else {
        ""
    };
    let dealer_marker = if app.dealer.0 == pid { " (dealer)" } else { "" };

    let info = format!("{}{}{}", chooser_marker, dealer_marker, turn_marker);

    let lines = vec![Line::from(card_line), Line::from(info)];

    let p = Paragraph::new(lines)
        .block(Block::default().title(title).borders(Borders::ALL))
        .alignment(align)
        .wrap(Wrap { trim: true });

    f.render_widget(p, area);
}

fn draw_bottom(f: &mut Frame, app: &mut App, area: Rect) {
    // Bottom split: hand (left) and logs (right)
    let cols = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(60), Constraint::Percentage(40)])
        .split(area);

    draw_hand(f, app, cols[0]);
    draw_logs(f, app, cols[1]);
}

fn draw_hand(f: &mut Frame, app: &mut App, area: Rect) {
    let hand = &app.game.hands[0];

    let items: Vec<ListItem> = hand
        .iter()
        .enumerate()
        .map(|(i, c)| {
            // cleaner: " 0: 4C"
            let label = format!("{:2}: {}", i, card_short(*c));
            ListItem::new(Line::from(label))
        })
        .collect();

    let title = match app.hand_state.selected() {
        Some(i) => format!("Your Hand (P0) - selected {}", i),
        None => "Your Hand (P0) - selected none".to_string(),
    };

    let list = List::new(items)
        .block(Block::default().title(title).borders(Borders::ALL))
        .highlight_style(Style::default().add_modifier(Modifier::REVERSED))
        .highlight_symbol(">> ");

    f.render_stateful_widget(list, area, &mut app.hand_state);
}

fn draw_logs(f: &mut Frame, app: &App, area: Rect) {
    let start = app.logs.len().saturating_sub(25);
    let view = &app.logs[start..];

    let items: Vec<ListItem> = view
        .iter()
        .map(|s| ListItem::new(Line::from(s.clone())))
        .collect();

    let list = List::new(items).block(Block::default().title("Logs").borders(Borders::ALL));

    f.render_widget(list, area);
}

// -------------------- helpers --------------------

fn inset(r: Rect, pad: u16) -> Rect {
    Rect {
        x: r.x + pad,
        y: r.y + pad,
        width: r.width.saturating_sub(pad * 2),
        height: r.height.saturating_sub(pad * 2),
    }
}

/// If player pid already played a card in current trick, return it as short string.
fn trick_card_for(app: &App, pid: u8) -> Option<String> {
    for (p, c) in &app.game.current_trick {
        if p.0 == pid {
            return Some(card_short(*c));
        }
    }
    None
}

/// Compact card formatting: "AS", "10H", "4C"
fn card_short(card: hokm_core::Card) -> String {
    let suit = match card.suit {
        hokm_core::Suit::Clubs => "C",
        hokm_core::Suit::Diamonds => "D",
        hokm_core::Suit::Hearts => "H",
        hokm_core::Suit::Spades => "S",
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
