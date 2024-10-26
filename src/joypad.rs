use enumset::EnumSetType;

#[derive(Debug, EnumSetType)]
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
