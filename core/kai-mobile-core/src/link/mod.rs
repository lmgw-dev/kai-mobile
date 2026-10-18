//! The link to lmgw (design §3.1, §4, §5, §12): the pinned TLS configuration and the one client
//! constructor ([`tls`]), the URL race ([`race`]), why a connection failed ([`failure`]) and what
//! the state then says ([`words`]), one request ([`http`]), the held feed's connection ([`feed`]),
//! and the actor that owns it all ([`actor`]).

pub mod actor;
pub mod failure;
pub mod feed;
pub mod http;
pub mod race;
pub mod tls;
pub mod words;
