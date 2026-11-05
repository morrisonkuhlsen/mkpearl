//! A Typst editor with a word processor's ribbon: source on the left, live pages on the right.
#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod app;
pub mod assistant;
pub mod bib;
pub mod carets;
pub mod collab;
pub mod colour_picker;
pub mod comments;
pub mod compare;
pub mod compiler;
pub mod doi;
pub mod edit;
pub mod fit;
pub mod formulas;
pub mod guide;
pub mod highlight;
pub mod history;
pub mod i18n;
pub mod icons;
pub mod images;
pub mod letterhead;
pub mod licenses;
pub mod recent;
pub mod recovery;
pub mod ruler;
pub mod sample;
pub mod spell;
pub mod splash;
pub mod templates;
pub mod theme;
pub mod timeline;
pub mod typing;
pub mod typography;
pub mod variants;
pub mod widgets;
pub mod worktime;
pub mod world;
