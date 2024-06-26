#[derive(PartialEq, Eq, PartialOrd, Ord)]
pub enum CardVal {
    N2,
    N3,
    N4,
    N5,
    N6,
    N7,
    N8,
    N9,
    N10,
    Jack,
    Queen,
    King,
    Ace,
}
#[derive(PartialEq, Eq)]

pub enum CardSuit {
    Club,
    Diamond,
    Heart,
    Spade,
}

pub struct Card {
    suit: CardSuit,
    val: CardVal,
}
