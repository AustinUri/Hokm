// Hokm/crates/hokm_core/src/lib.rs
//
// This crate is ONLY game rules.
// No UI.
// No networking.
// No timers.
// No XP.
// No randomness (yet).
//
// Think of it like: "the referee".

#![forbid(unsafe_code)] // beginner-friendly: avoid unsafe code

use std::fmt;
mod deal;
pub use deal::*;

/// PlayerId is the seat number:
/// 0, 1, 2, 3
///
/// Teams are fixed:
/// Team A = players 0 and 2
/// Team B = players 1 and 3
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PlayerId(pub u8);

impl PlayerId {
    /// Convert PlayerId into an index for arrays (0..4)
    pub fn idx(self) -> usize {
        self.0 as usize
    }

    /// Which team is this player on?
    pub fn team(self) -> Team {
        match self.0 % 2 {
            0 => Team::A, // even seats: 0 and 2
            _ => Team::B, // odd seats: 1 and 3
        }
    }

    /// The next player clockwise.
    pub fn next(self) -> PlayerId {
        PlayerId((self.0 + 1) % 4)
    }
}

/// Two teams in Hokm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Team {
    A,
    B,
}

impl Team {
    /// Convert team into array index (Team A = 0, Team B = 1)
    pub fn idx(self) -> usize {
        match self {
            Team::A => 0,
            Team::B => 1,
        }
    }

    /// The other team (A <-> B)
    pub fn other(self) -> Team {
        match self {
            Team::A => Team::B,
            Team::B => Team::A,
        }
    }
}

/// Card suits.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Suit {
    Clubs,
    Diamonds,
    Hearts,
    Spades,
}

/// Card rank order.
/// Bigger value = stronger card.
/// Ace is strongest.
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

/// A playing card.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Card {
    pub suit: Suit,
    pub rank: Rank,
}

/// The game phase (what is allowed right now).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// Someone must choose the hokm (trump suit).
    ChoosingHokm { chooser: PlayerId },

    /// Players are playing tricks.
    Playing,

    /// A round ended (someone got 7 tricks).
    /// Kot is computed ONLY when round winner exists.
    RoundOver { winner: Team, kot: bool },

    /// Whole match ended (someone won 7 rounds).
    GameOver { winner: Team },
}

/// Actions are things a PLAYER can decide to do.
///
/// Note: Dealing is NOT a player action.
/// The server/offline "system" does dealing between rounds.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    ChooseHokm { suit: Suit },
    PlayCard { card: Card },
}

/// Events are what happened because of an action.
/// UI/network/XP can react to events.
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

    /// Round ended and we know if it was Kot.
    /// Kot definition: losing team took 0 tricks.
    RoundEnded {
        winner: Team,
        kot: bool,
    },

    GameEnded {
        winner: Team,
    },
}

/// Error = "you tried something illegal"
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

/// Full game state.
/// This is the "truth" version (server/offline).
///
/// In multiplayer, clients must NOT get enemy hands.
/// Later we will make `GameView` for that.
#[derive(Clone, Debug)]
pub struct GameState {
    pub phase: Phase,

    /// Who dealt this round.
    pub dealer: PlayerId,

    /// Whose turn it is now.
    pub turn: PlayerId,

    /// Trump suit (hokm). None until chosen.
    pub hokm: Option<Suit>,

    /// Everyone's hands (hidden info!).
    pub hands: [Vec<Card>; 4],

    /// Current trick cards: (player, card) in play order.
    pub current_trick: Vec<(PlayerId, Card)>,

    /// Tricks taken THIS ROUND: [Team A, Team B]
    pub tricks_taken: [u8; 2],

    /// Rounds won THIS MATCH: [Team A, Team B]
    /// Match winner = first team to 7 rounds.
    pub rounds_won: [u8; 2],
}

impl GameState {
    /// Create a new game state with already-dealt hands.
    ///
    /// For now we don't shuffle/deal here yet.
    /// This is perfect for tests and early prototyping.
    pub fn new_with_hands(dealer: PlayerId, hands: [Vec<Card>; 4]) -> Self {
        // Simplest policy: dealer chooses hokm.
        // If you later want dealer's partner or something else, change here.
        let chooser = dealer;

        Self {
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

    /// Returns all legal actions for `player` right now.
    ///
    /// Useful for:
    /// - UI to show what you can click/play
    /// - AI to choose only legal moves
    pub fn legal_actions(&self, player: PlayerId) -> Vec<Action> {
        // Not your turn? Nothing is legal.
        if player != self.turn {
            return vec![];
        }

        match self.phase {
            // Choosing Hokm: chooser can pick any suit.
            Phase::ChoosingHokm { chooser } if chooser == player => vec![
                Action::ChooseHokm { suit: Suit::Clubs },
                Action::ChooseHokm {
                    suit: Suit::Diamonds,
                },
                Action::ChooseHokm { suit: Suit::Hearts },
                Action::ChooseHokm { suit: Suit::Spades },
            ],

            // Playing: you can play a card but must follow suit if you can.
            Phase::Playing => {
                let hand = &self.hands[player.idx()];
                if hand.is_empty() {
                    return vec![];
                }

                // If trick is empty, you can lead anything.
                if self.current_trick.is_empty() {
                    return hand
                        .iter()
                        .copied()
                        .map(|card| Action::PlayCard { card })
                        .collect();
                }

                // Trick already has lead card => must follow lead suit if possible.
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

            // RoundOver/GameOver: no player actions in core.
            _ => vec![],
        }
    }

    /// Apply a player's action to the game state.
    ///
    /// This does:
    /// 1) validate turn and rules
    /// 2) update game state
    /// 3) return events describing what happened
    pub fn apply_action(&mut self, player: PlayerId, action: Action) -> Result<Vec<Event>, Error> {
        // 0) Turn check
        if player != self.turn {
            return Err(Error::NotPlayersTurn);
        }

        match (self.phase, action) {
            // =======================
            // CHOOSING HOKM
            // =======================
            (Phase::ChoosingHokm { chooser }, Action::ChooseHokm { suit }) => {
                if chooser != player {
                    return Err(Error::WrongPhase);
                }
                if self.hokm.is_some() {
                    return Err(Error::HokmAlreadyChosen);
                }

                // Set hokm (trump)
                self.hokm = Some(suit);

                // Move to playing
                self.phase = Phase::Playing;

                // Common rule: first player is left of dealer
                self.turn = self.dealer.next();

                Ok(vec![Event::HokmChosen { suit }])
            }

            // =======================
            // PLAYING A CARD
            // =======================
            (Phase::Playing, Action::PlayCard { card }) => {
                // 1) Player must have the card
                let hand = &mut self.hands[player.idx()];
                let pos = hand
                    .iter()
                    .position(|&c| c == card)
                    .ok_or(Error::CardNotInHand)?;

                // 2) Must-follow-suit rule
                if !self.current_trick.is_empty() {
                    let lead_suit = self.current_trick[0].1.suit;

                    // If you try to play a different suit...
                    if card.suit != lead_suit {
                        // ...but you actually HAVE the lead suit in hand => illegal
                        let has_lead = hand.iter().any(|c| c.suit == lead_suit);
                        if has_lead {
                            return Err(Error::MustFollowSuit {
                                required: lead_suit,
                            });
                        }
                    }
                }

                // 3) Remove card from hand
                // swap_remove is fast but changes the order of hand.
                // That is fine: order does not matter.
                hand.swap_remove(pos);

                // 4) Add it to the trick
                self.current_trick.push((player, card));

                let mut events = vec![Event::CardPlayed { player, card }];

                // 5) If we don't have 4 cards yet, next player's turn
                if self.current_trick.len() < 4 {
                    self.turn = self.turn.next();
                    return Ok(events);
                }

                // 6) Trick finished. Find winner.
                let hokm = self.hokm.expect("hokm must be chosen before playing");
                let winner = trick_winner(&self.current_trick, hokm);
                events.push(Event::TrickEnded { winner });

                // 7) Winner team gets +1 trick
                let wteam = winner.team();
                self.tricks_taken[wteam.idx()] += 1;

                // 8) Clear trick and winner leads next trick
                self.current_trick.clear();
                self.turn = winner;

                // 9) Round end check: first to 7 tricks wins the round.
                // We check Kot ONLY here, because only now do we know the round winner.
                if self.tricks_taken[wteam.idx()] >= 7 {
                    let loser_team = wteam.other();

                    // Kot rule (your rule):
                    // kot = loser took 0 tricks in the round
                    let kot = self.tricks_taken[loser_team.idx()] == 0;

                    // Winner gets +1 round
                    self.rounds_won[wteam.idx()] += 1;

                    events.push(Event::RoundEnded { winner: wteam, kot });

                    // 10) Match end check: first to 7 rounds wins the match.
                    if self.rounds_won[wteam.idx()] >= 7 {
                        self.phase = Phase::GameOver { winner: wteam };
                        events.push(Event::GameEnded { winner: wteam });
                    } else {
                        self.phase = Phase::RoundOver { winner: wteam, kot };
                    }
                }

                Ok(events)
            }

            // Anything else is illegal right now
            _ => Err(Error::WrongPhase),
        }
    }

    /// Start a new round AFTER RoundOver.
    ///
    /// We reset:
    /// - hokm
    /// - current trick
    /// - tricks_taken
    /// and we replace hands with new dealt hands.
    ///
    /// Dealer moves to next dealer outside this function (you pass it in).
    pub fn start_next_round(&mut self, new_dealer: PlayerId, new_hands: [Vec<Card>; 4]) {
        self.dealer = new_dealer;
        self.hands = new_hands;

        self.hokm = None;
        self.current_trick.clear();
        self.tricks_taken = [0, 0];

        // Dealer chooses hokm in our policy
        self.phase = Phase::ChoosingHokm {
            chooser: new_dealer,
        };
        self.turn = new_dealer;
    }
}

/// Decide who wins a trick.
///
/// Inputs:
/// - trick: list of (player, card) in play order (length 4)
/// - hokm: trump suit
///
/// Rules:
/// - Lead suit = suit of first card.
/// - Any trump (hokm) beats any non-trump.
/// - Among trump cards: highest rank wins.
/// - If no trump: highest rank in lead suit wins.
/// - Off-suit non-trump cannot win.
fn trick_winner(trick: &[(PlayerId, Card)], hokm: Suit) -> PlayerId {
    let lead_suit = trick[0].1.suit;

    // Start by assuming first card is best
    let mut best = trick[0];

    for &(pid, card) in trick.iter().skip(1) {
        let best_card = best.1;

        let card_is_trump = card.suit == hokm;
        let best_is_trump = best_card.suit == hokm;

        let card_is_lead = card.suit == lead_suit;
        let best_is_lead = best_card.suit == lead_suit;

        // Decide if current card beats best card
        let wins = if card_is_trump && !best_is_trump {
            true
        } else if !card_is_trump && best_is_trump {
            false
        } else if card_is_trump && best_is_trump {
            // both trump => higher rank wins
            card.rank > best_card.rank
        } else {
            // no trump in comparison => only lead suit matters
            match (card_is_lead, best_is_lead) {
                (true, false) => true,
                (false, true) => false,
                (true, true) => card.rank > best_card.rank,
                (false, false) => false, // off-suit can't win
            }
        };

        if wins {
            best = (pid, card);
        }
    }

    best.0
}
