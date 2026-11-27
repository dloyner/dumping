//! basic_web_security: a small study project that turns the core web security
//! concepts into runnable, tested Rust code. One module per concept.
//!
//! Big picture (read in this order):
//!   xss -> csrf -> site (same-origin vs same-site) -> cors -> headers (clickjacking)
//!   -> session (cookie flags) -> ssrf -> dos -> n_plus_one -> rest (CRUD)

pub mod cors;
pub mod csrf;
pub mod dos;
pub mod headers;
pub mod n_plus_one;
pub mod rest;
pub mod session;
pub mod site;
pub mod ssrf;
pub mod xss;
