//! CMD Studio core: theme model, palette, generators, planner and the
//! transactional apply engine. Nothing here touches the machine directly;
//! side effects go through the traits in `txn`.
pub mod color;
pub mod generate;
pub mod model;
pub mod oklch;
pub mod palette;
pub mod plan;
pub mod presets;
pub mod schema;
pub mod text;
pub mod txn;
