//! Shared core of the singing trainer: music theory, the session/exercise schema, deterministic
//! trial generation, pitch analysis, grading, storage and statistics. Used by the desktop app and
//! by the MCP server, so both always agree on what an exercise is and how it is graded.

pub mod ap_courses;
pub mod ap_tests;
pub mod aptitude;
pub mod audiation_courses;
pub mod courses;
pub mod generate;
pub mod grade;
pub mod labels;
pub mod library;
pub mod melodia_course;
pub mod melody;
pub mod melody_ladder;
pub mod metrics;
pub mod pitch;
pub mod presets;
pub mod reports;
pub mod rng;
pub mod rp_courses;
pub mod schema;
pub mod score;
pub mod speech;
pub mod stats;
pub mod storage;
pub mod theory;
pub mod timbre;
pub mod validate;
pub mod workshops;

pub use generate::{GenContext, block_seed, default_mode, generate_block};
pub use grade::{GradingConfig, TrialOutcome};
pub use schema::*;
pub use storage::Db;
