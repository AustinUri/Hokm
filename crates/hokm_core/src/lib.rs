// Hokm/crates/hokm_core/src/lib.rs
//
// hokm_core = pure game rules.
// No UI, no networking, no async, no XP.
//
// This file defines:
// - types (cards, players, teams)
// - state machine (GameState + Phase)
// - actions (Action)
// - rules enforcement (legal_actions + apply_action)
// - events (Event) so UI/network/XP can react
// - config (GameConfig) so match length can be changed (default 7)

#![forbid(unsafe_code)]

mod deal;
pub use deal::*;

use std::fmt;

/// PlayerId is seat number: 0,1,2,3
/// Teams are fixed: (0&2) vs (1&3)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u8);

impl PlayerId {
    pub fn idx(self) -> usize {
        self.0 as usize
    }

    pub fn team(self) -> Team {
        match self.0 % 2 {
            0 => Team::A,
            _ => Team::B,
        }
    }

    pub fn next(self) -> PlayerId {
        PlayerId((self.0 + 1) % 4)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    A,
    B,
}

impl Team {
    pub fn idx(self) -> usize {
        match self {
            Team::A => 0,
            Team::B => 1,
        }
    }

    pub fn other(self) -> Team {
        match self {
            Team::A => Team::B,
            Team::B => Team::A,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Rank {
    Two = 2,
    Three = 3,
    Four = 4,
    Five = 5,
    Six = 6,
    Seven = 7,
    Eight = 8,
    Nine = 9,
    Ten = 10,
    Jack = 11,
    Queen = 12,
    King = 13,
    Ace = 14,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

/// GameConfig contains tunable knobs for a match.
/// Keep it in core so server and offline both follow the same rules.
#[derive(Clone, Copy, Debug)]
pub struct GameConfig {
    /// How many ROUND wins are needed to win the whole match.
    /// Classic Hokm is 7.
    pub target_round_wins: u8,
}

impl Default for GameConfig {
    fn default() -> Self {
        Self {
            target_round_wins: 7,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    ChoosingHokm {
        chooser: PlayerId,
    },
    Playing,
    /// RoundOver means a team reached 7 tricks.
    /// kot is computed ONLY here because kot is a round result.
    RoundOver {
        winner: Team,
        kot: bool,
    },
    GameOver {
        winner: Team,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ChooseHokm { suit: Suit },
    PlayCard { card: Card },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    HokmChosen {
        suit: Suit,
    },
    CardPlayed {
        player: PlayerId,
        card: Card,
    },
    TrickEnded {
        winner: PlayerId,
    },
    /// Kot definition: losing team took 0 tricks (checked when round winner exists)
    RoundEnded {
        winner: Team,
        kot: bool,
    },
    GameEnded {
        winner: Team,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    NotPlayersTurn,
    WrongPhase,
    CardNotInHand,
    MustFollowSuit { required: Suit },
    HokmAlreadyChosen,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        use Error::*;
        match self {
            NotPlayersTurn => write!(f, "not your turn"),
            WrongPhase => write!(f, "wrong phase for this action"),
            CardNotInHand => write!(f, "that card is not in your hand"),
            MustFollowSuit { required } => write!(f, "must follow suit: {:?}", required),
            HokmAlreadyChosen => write!(f, "hokm already chosen"),
        }
    }
}

/// Full truth state (server/offline). Clients will later receive a filtered view.
#[derive(Clone, Debug)]
pub struct GameState {
    /// Config stays the same for the whole match.
    pub config: GameConfig,

    pub phase: Phase,
    pub dealer: PlayerId,
    pub turn: PlayerId,
    pub hokm: Option<Suit>,

    /// Hidden info: all hands.
    pub hands: [Vec<Card>; 4],

    /// Current trick, in play order (0..4 cards).
    pub current_trick: Vec<(PlayerId, Card)>,

    /// Tricks in THIS ROUND: [Team A, Team B]
    pub tricks_taken: [u8; 2],

    /// Rounds in THIS MATCH: [Team A, Team B]
    pub rounds_won: [u8; 2],
}

impl GameState {
    /// Create a new state using DEFAULT config (target_round_wins=7).
    pub fn new_with_hands(dealer: PlayerId, hands: [Vec<Card>; 4]) -> Self {
        Self::new_with_hands_config(dealer, hands, GameConfig::default())
    }

    /// Create a new state with custom config.
    /// Example: target_round_wins=1 for quick testing.
    pub fn new_with_hands_config(
        dealer: PlayerId,
        hands: [Vec<Card>; 4],
        config: GameConfig,
    ) -> Self {
        let chooser: PlayerId = dealer;

        Self {
            config,
            phase: Phase::ChoosingHokm { chooser },
            dealer,
            turn: chooser,
            hokm: None,
            hands,
            current_trick: Vec::with_capacity(4),
            tricks_taken: [0, 0],
            rounds_won: [0, 0],
        }
    }

    /// Return legal actions for the given player.
    /// If it's not their turn, returns empty.
    pub fn legal_actions(&self, player: PlayerId) -> Vec<Action> {
        if player != self.turn {
            return vec![];
        }

        match self.phase {
            Phase::ChoosingHokm { chooser } if chooser == player => vec![
                Action::ChooseHokm { suit: Suit::Clubs },
                Action::ChooseHokm {
                    suit: Suit::Diamonds,
                },
                Action::ChooseHokm { suit: Suit::Hearts },
                Action::ChooseHokm { suit: Suit::Spades },
            ],

            Phase::Playing => {
                let hand = &self.hands[player.idx()];
                if hand.is_empty() {
                    return vec![];
                }

                // If trick is empty, you can lead any card.
                if self.current_trick.is_empty() {
                    return hand
                        .iter()
                        .copied()
                        .map(|card| Action::PlayCard { card })
                        .collect();
                }

                // Must follow lead suit if possible.
                let lead_suit = self.current_trick[0].1.suit;
                let has_lead_suit = hand.iter().any(|c| c.suit == lead_suit);

                if has_lead_suit {
                    hand.iter()
                        .copied()
                        .filter(|c| c.suit == lead_suit)
                        .map(|card| Action::PlayCard { card })
                        .collect()
                } else {
                    hand.iter()
                        .copied()
                        .map(|card| Action::PlayCard { card })
                        .collect()
                }
            }

            _ => vec![],
        }
    }

    /// Apply an action from the current-turn player.
    pub fn apply_action(&mut self, player: PlayerId, action: Action) -> Result<Vec<Event>, Error> {
        if player != self.turn {
            return Err(Error::NotPlayersTurn);
        }

        match (self.phase, action) {
            // ---- Choose Hokm ----
            (Phase::ChoosingHokm { chooser }, Action::ChooseHokm { suit }) => {
                if chooser != player {
                    return Err(Error::WrongPhase);
                }
                if self.hokm.is_some() {
                    return Err(Error::HokmAlreadyChosen);
                }

                self.hokm = Some(suit);
                self.phase = Phase::Playing;

                // First player is left of dealer (common).
                self.turn = self.dealer.next();

                Ok(vec![Event::HokmChosen { suit }])
            }

            // ---- Play Card ----
            (Phase::Playing, Action::PlayCard { card }) => {
                // 1) Make sure the player has that card.
                let hand = &mut self.hands[player.idx()];
                let pos = hand
                    .iter()
                    .position(|&c| c == card)
                    .ok_or(Error::CardNotInHand)?;

                // 2) Follow suit if possible.
                if !self.current_trick.is_empty() {
                    let lead_suit = self.current_trick[0].1.suit;

                    if card.suit != lead_suit {
                        let has_lead = hand.iter().any(|c| c.suit == lead_suit);
                        if has_lead {
                            return Err(Error::MustFollowSuit {
                                required: lead_suit,
                            });
                        }
                    }
                }

                // 3) Remove from hand (fast).
                hand.swap_remove(pos);

                // 4) Add to trick.
                self.current_trick.push((player, card));

                let mut events = vec![Event::CardPlayed { player, card }];

                // 5) If trick not complete, next turn.
                if self.current_trick.len() < 4 {
                    self.turn = self.turn.next();
                    return Ok(events);
                }

                // 6) Trick complete -> find winner.
                let hokm = self.hokm.expect("hokm must be chosen before playing");
                let winner = trick_winner(&self.current_trick, hokm);
                events.push(Event::TrickEnded { winner });

                // 7) Update tricks.
                let wteam = winner.team();
                self.tricks_taken[wteam.idx()] += 1;

                // 8) Winner leads next trick.
                self.current_trick.clear();
                self.turn = winner;

                // 9) Round end when a team reaches 7 tricks.
                // Kot is computed ONLY here (winner exists).
                if self.tricks_taken[wteam.idx()] >= 7 {
                    let loser_team = wteam.other();
                    let kot = self.tricks_taken[loser_team.idx()] == 0;

                    self.rounds_won[wteam.idx()] += 1;
                    events.push(Event::RoundEnded { winner: wteam, kot });

                    // 10) Match end when a team reaches config.target_round_wins.
                    if self.rounds_won[wteam.idx()] >= self.config.target_round_wins {
                        self.phase = Phase::GameOver { winner: wteam };
                        events.push(Event::GameEnded { winner: wteam });
                    } else {
                        self.phase = Phase::RoundOver { winner: wteam, kot };
                    }
                }

                Ok(events)
            }

            _ => Err(Error::WrongPhase),
        }
    }

    /// Start next round (after RoundOver), using new dealer and new dealt hands.
    /// Config and rounds_won stay.
    pub fn start_next_round(&mut self, new_dealer: PlayerId, new_hands: [Vec<Card>; 4]) {
        self.dealer = new_dealer;
        self.hands = new_hands;

        self.hokm = None;
        self.current_trick.clear();
        self.tricks_taken = [0, 0];

        let chooser = new_dealer;
        self.phase = Phase::ChoosingHokm { chooser };
        self.turn = chooser;
    }
}

/// Determine who won the trick.
/// - lead suit = first card's suit
/// - hokm beats non-hokm
/// - compare ranks within the same winning category
fn trick_winner(trick: &[(PlayerId, Card)], hokm: Suit) -> PlayerId {
    let lead_suit = trick[0].1.suit;
    let mut best = trick[0];

    for &(pid, card) in trick.iter().skip(1) {
        let best_card = best.1;

        let card_is_trump = card.suit == hokm;
        let best_is_trump = best_card.suit == hokm;

        let card_is_lead = card.suit == lead_suit;
        let best_is_lead = best_card.suit == lead_suit;

        let wins = if card_is_trump && !best_is_trump {
            true
        } else if !card_is_trump && best_is_trump {
            false
        } else if card_is_trump && best_is_trump {
            card.rank > best_card.rank
        } else {
            match (card_is_lead, best_is_lead) {
                (true, false) => true,
                (false, true) => false,
                (true, true) => card.rank > best_card.rank,
                (false, false) => false,
            }
        };

        if wins {
            best = (pid, card);
        }
    }

    best.0
}
