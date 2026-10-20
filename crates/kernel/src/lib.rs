//! @managed rust-hex-api — общее ядро (shared kernel). Фичи зависят только от него, но не друг от друга.
pub mod auth;
pub mod code;
pub mod config;
pub mod context;
pub mod crypto;
pub mod error;
pub mod feature;
pub mod i18n;
pub mod pagination;
pub mod rates;
pub mod search;
pub mod testing;
pub mod throttle;
pub mod undo;
pub mod web;
