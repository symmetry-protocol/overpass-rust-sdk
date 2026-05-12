# overpass-rust-sdk

Rust SDK for the [Overpass program](https://github.com/symmetry-protocol/overpass-program).
Implements the Jupiter `Amm` trait for Overpass wrapper vaults across all five
supported source protocols (Kamino Lend, Save Lend, Kamino Vault, MarginFi,
Lulo Protected). On-chain math is mirrored byte-for-byte from the program's
instruction handlers; per-protocol math modules are dependency-free and
intended for direct port to TypeScript in a later phase.
