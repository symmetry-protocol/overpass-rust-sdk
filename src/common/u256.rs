#[allow(
    clippy::assign_op_pattern,
    clippy::manual_div_ceil,
    clippy::double_parens,
    clippy::reversed_empty_ranges
)]
mod uint_types {
    uint::construct_uint! {
        pub struct U256(4);
    }
}
pub use uint_types::U256;
