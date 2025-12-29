//! FleetFlow Cloud Infrastructure
//!
//! This crate provides cloud provider abstraction for FleetFlow,
//! enabling declarative management of cloud resources across multiple providers.
//!
//! # Supported Providers
//!
//! - **Sakura Cloud**: Servers, Disks (via usacloud CLI)
//! - **Cloudflare**: R2, Workers, DNS (via wrangler CLI and API)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────┐
//! │                  FleetFlow CLI                   │
//! │                  (fleet up/down)                  │
//! └─────────────────┬───────────────────────────────┘
//!                   │
//! ┌─────────────────▼───────────────────────────────┐
//! │               fleetflow-cloud                    │
//! │  ┌──────────────────────────────────────────┐   │
//! │  │          Provider Abstraction             │   │
//! │  │  trait CloudProvider { ... }              │   │
//! │  └──────────────────────────────────────────┘   │
//! │  ┌──────────────┐  ┌──────────────┐            │
//! │  │  KDL Parser  │  │  State Mgmt  │            │
//! │  └──────────────┘  └──────────────┘            │
//! └───────┬─────────────────┬───────────────────────┘
//!         │                 │
//! ┌───────▼───────┐ ┌───────▼───────┐
//! │ sakura-cloud  │ │  cloudflare   │
//! │   provider    │ │   provider    │
//! └───────────────┘ └───────────────┘
//! ```

pub mod action;
pub mod error;
pub mod provider;
pub mod state;

// Re-exports
pub use action::{Action, ActionType, ApplyResult, Plan, PlanSummary};
pub use error::{CloudError, Result};
pub use provider::{AuthStatus, CloudProvider, ResourceConfig, ResourceSet, RetryConfig};
pub use state::{
    GlobalState, ProviderState, ResourceState, ResourceStatus, StateLock, StateManager,
};
