use thiserror::Error;

#[derive(Error, Debug)]
pub enum OverpassError {
    #[error("math overflow")]
    MathOverflow,

    #[error("invalid account: {0}")]
    InvalidAccount(&'static str),

    #[error("invalid state: {0}")]
    InvalidState(&'static str),

    #[error("account missing from update map: {0}")]
    AccountMissing(&'static str),

    #[error("wrapper protocol mismatch")]
    ProtocolMismatch,

    #[error("unsupported swap mode")]
    UnsupportedSwapMode,

    #[error("invalid amount")]
    InvalidAmount,
    #[error("insufficient liquidity")]
    InsufficientLiquidity,
    #[error("deposit limit exceeded")]
    DepositLimitExceeded,
    #[error("burn amount exceeds wrapper supply")]
    BurnExceedsWrapperSupply,

    #[error("klend: reserve obsolete")]
    ReserveObsolete,
    #[error("klend: reserve deprecated (version mismatch)")]
    ReserveDeprecated,
    #[error("klend: emergency mode")]
    GlobalEmergencyMode,
    #[error("klend: cToken usage blocked")]
    CTokenUsageBlocked,
    #[error("klend: withdrawal cap reached")]
    WithdrawalCapReached,

    #[error("marginfi: bank killed by bankruptcy")]
    BankKilled,
    #[error("marginfi: bank paused")]
    BankPaused,
    #[error("marginfi: bank reduce-only (deposits blocked)")]
    BankReduceOnly,
    #[error("marginfi: deposit cap exceeded")]
    BankAssetCapacityExceeded,
}
