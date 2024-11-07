use enumset::EnumSetType;
use serde::Serialize;

#[derive(Debug, EnumSetType, Serialize)]
#[enumset(repr = "u8")]
pub enum Button {
    A,
    B,
    Start,
    Select,
    Up,
    Down,
    Left,
    Right,
}
