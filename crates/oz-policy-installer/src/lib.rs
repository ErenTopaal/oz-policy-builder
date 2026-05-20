//! Install-envelope builder for OZ `SmartAccount::add_context_rule` /
//! `add_policy`.
//!
//! Phase 2 Stream B scaffold. The first landed module is [`registry`],
//! which keeps the network-keyed table of OZ primitive contract
//! addresses (see that module's doc-comment for why every entry is
//! `None` in v1). [`envelope`] and [`preflight`] land in follow-up
//! commits.

#![forbid(unsafe_code)]

pub mod registry;
