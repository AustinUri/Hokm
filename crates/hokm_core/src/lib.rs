// Hokm/crates/hokm_core/src/lib.rs
//
// Pure rules engine. No UI. No networking. No XP.
// Enforces move legality, trick winners, round end at 7 tricks, Kot detection.

#![forbid(unsafe_code)]

mod deal;
pub use deal::*;

use std::fmt;

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

#[derive(Clone, Copy, Debug)]
pub struct GameConfig {
    /// How many ROUND wins to win the whole match (default 7).
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
    ChoosingHokm { chooser: PlayerId },
    Playing,
    RoundOver { winner: Team, kot: bool },
    GameOver { winner: Team },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ChooseHokm { suit: Suit },
    PlayCard { card: Card },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    HokmChosen { suit: Suit },
    CardPlayed { player: PlayerId, card: Card },
    TrickEnded { winner: PlayerId },
    RoundEnded { winner: Team, kot: bool },
    GameEnded { winner: Team },
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

#[derive(Clone, Debug)]
pub struct GameState {
    pub config: GameConfig,

    pub phase: Phase,
    pub dealer: PlayerId,
    pub turn: PlayerId,
    pub hokm: Option<Suit>,

    pub hands: [Vec<Card>; 4],
    pub current_trick: Vec<(PlayerId, Card)>,

    pub tricks_taken: [u8; 2],
    pub rounds_won: [u8; 2],
}

impl GameState {
    /// Old convenience: dealer chooses hokm (not your final rule).
    pub fn new_with_hands(dealer: PlayerId, hands: [Vec<Card>; 4]) -> Self {
        Self::new_with_hands_config(dealer, hands, GameConfig::default())
    }

    /// Old convenience: dealer chooses hokm (not your final rule).
    pub fn new_with_hands_config(
        dealer: PlayerId,
        hands: [Vec<Card>; 4],
        config: GameConfig,
    ) -> Self {
        let chooser = dealer;
        Self::new_round_with_hands_config(dealer, chooser, hands, config)
    }

    /// ✅ Your correct rule constructor:
    /// dealer is dealer, chooser is chooser (first Ace winner).
    pub fn new_round_with_hands_config(
        dealer: PlayerId,
        chooser: PlayerId,
        hands: [Vec<Card>; 4],
        config: GameConfig,
    ) -> Self {
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

                if self.current_trick.is_empty() {
                    return hand
                        .iter()
                        .copied()
                        .map(|card| Action::PlayCard { card })
                        .collect();
                }

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

    pub fn apply_action(&mut self, player: PlayerId, action: Action) -> Result<Vec<Event>, Error> {
        if player != self.turn {
            return Err(Error::NotPlayersTurn);
        }

        match (self.phase, action) {
            (Phase::ChoosingHokm { chooser }, Action::ChooseHokm { suit }) => {
                if chooser != player {
                    return Err(Error::WrongPhase);
                }
                if self.hokm.is_some() {
                    return Err(Error::HokmAlreadyChosen);
                }

                self.hokm = Some(suit);
                self.phase = Phase::Playing;

                // First play starts left of dealer (common).
                self.turn = self.dealer.next();

                Ok(vec![Event::HokmChosen { suit }])
            }

            (Phase::Playing, Action::PlayCard { card }) => {
                let hand = &mut self.hands[player.idx()];
                let pos = hand
                    .iter()
                    .position(|&c| c == card)
                    .ok_or(Error::CardNotInHand)?;

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

                hand.swap_remove(pos);
                self.current_trick.push((player, card));

                let mut events = vec![Event::CardPlayed { player, card }];

                if self.current_trick.len() < 4 {
                    self.turn = self.turn.next();
                    return Ok(events);
                }

                let hokm = self.hokm.expect("hokm must be chosen before playing");
                let winner = trick_winner(&self.current_trick, hokm);
                events.push(Event::TrickEnded { winner });

                let wteam = winner.team();
                self.tricks_taken[wteam.idx()] += 1;

                self.current_trick.clear();
                self.turn = winner;

                // Round ends when team hits 7 tricks.
                if self.tricks_taken[wteam.idx()] >= 7 {
                    let loser_team = wteam.other();
                    // ✅ Kot = loser took 0 tricks, computed ONLY when round winner exists.
                    let kot = self.tricks_taken[loser_team.idx()] == 0;

                    self.rounds_won[wteam.idx()] += 1;
                    events.push(Event::RoundEnded { winner: wteam, kot });

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

    pub fn start_next_round(
        &mut self,
        new_dealer: PlayerId,
        new_hands: [Vec<Card>; 4],
        chooser: PlayerId,
    ) {
        self.dealer = new_dealer;
        self.hands = new_hands;

        self.hokm = None;
        self.current_trick.clear();
        self.tricks_taken = [0, 0];

        self.phase = Phase::ChoosingHokm { chooser };
        self.turn = chooser;
    }
}

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
