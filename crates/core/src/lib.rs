//! CMD Studio core: theme model, palette, generators, planner, the transactional apply
//! engine, and what runs it on a real machine. Only `adapters::windows` touches the machine;
//! everything else goes through the adapter traits in `adapters`.
pub mod actions;
pub mod adapters;
pub mod cleanup;
pub mod color;
pub mod detect;
pub mod doctor;
pub mod exec;
pub mod generate;
pub mod model;
pub mod oklch;
pub mod palette;
pub mod plan;
pub mod presets;
pub mod resources;
pub mod schema;
pub mod snapshot;
pub mod state;
pub mod text;
pub mod txn;
pub mod verify;
pub mod wt_edit;
