//! CMD Studio core: theme model, palette, generators, planner and the
//! transactional apply engine. Nothing here touches the machine directly;
//! side effects go through the traits in `txn`.
pub mod color;
