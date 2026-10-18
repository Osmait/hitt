//! Handler modules for `Core::handle`. Each module owns a slice of the
//! command space (collections, environments, config, HTTP requests) and
//! returns `Vec<CoreEvent>` — identical pattern to sbql's handlers.

pub(crate) mod collection;
pub(crate) mod config;
pub(crate) mod environment;
pub(crate) mod request;
pub(crate) mod testing;
