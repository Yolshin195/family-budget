//! @managed rust-hex-api — сборка фич и связки между ними (composition root).
//! Блоки между маркерами @arch:* ведёт `arch.py`: каждая строка помечена `// @f:<фича>`.
//! Связки (адаптеры портов «потребитель → поставщик») лежат в wire_<потребитель>.rs.

#[allow(unused_imports)]
use std::sync::Arc;

use kernel::context::AppContext;
use kernel::feature::Feature;

// @arch:mods:begin
// @arch:mods:end

#[rustfmt::skip]
pub fn features(ctx: &AppContext) -> Vec<Feature> {
    #[allow(unused_mut)]
    let mut list: Vec<Feature> = Vec::new();
    let _ = ctx;
    // @arch:build:begin
    // @arch:build:end
    list
}
