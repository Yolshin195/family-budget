//! @managed rust-hex-api — общее ядро (shared kernel). Фичи зависят только от него, но не друг от друга.
pub mod auth;
pub mod code;
pub mod context;
pub mod error;
pub mod feature;
pub mod pagination;
