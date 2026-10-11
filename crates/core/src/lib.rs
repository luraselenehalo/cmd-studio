//! CMD Studio core: theme model, palette, generators, planner and the
//! transactional apply engine. Nothing here touches the machine directly;
//! side effects go through the traits in `txn`.
pub mod color;
pub mod model;
pub mod oklch;
pub mod palette;
pub mod schema;
pub mod text;
