//! Domain layer: disc identity (serial), catalogue and PSX naming. No SDL.

pub mod catalog;
pub mod cheats;
pub mod disc;
pub mod infos;
pub mod library;
pub mod nointro;
pub mod rom;

pub use catalog::{Catalog, CatalogEntry, CatalogError, Order, RomRow};
pub use cheats::{for_title as cheats_for_title, CheatDef};
pub use disc::{normalize_serial, DiscError, DiscId};
pub use infos::InfosData;
pub use library::{scan, ScannedDisc, DISC_EXTS, PLAYLIST_EXTS, UNSUPPORTED_EXTS};
pub use nointro::{NoIntroDat, NoIntroError};
pub use rom::{Mapper, RomError, RomId};
