//! Sakura Cloud provider for FleetFlow
//!
//! This crate implements the CloudProvider trait for Sakura Cloud,
//! enabling FleetFlow to manage servers and disks on Sakura Cloud.
//!
//! # Features
//!
//! - Server management (create, delete, power on/off)
//! - Disk management
//! - SSH key management
//!
//! # Requirements
//!
//! - `usacloud` CLI must be installed and configured
//! - Authentication is managed through usacloud configuration
//!
//! # Example
//!
//! ```ignore
//! use fleetflow_cloud_sakura::SakuraCloudProvider;
//! use fleetflow_cloud::CloudProvider;
//!
//! let provider = SakuraCloudProvider::new("tk1a");
//!
//! // Check authentication
//! let auth = provider.check_auth().await?;
//! if !auth.authenticated {
//!     panic!("Not authenticated: {:?}", auth.error);
//! }
//!
//! // Get current state
//! let state = provider.get_state().await?;
//! ```

pub mod error;
pub mod provider;
pub mod startup_scripts;
pub mod usacloud;

pub use error::{Result, SakuraError};
pub use provider::{CreateServerOptions, SakuraCloudProvider, SimpleServerInfo};
pub use startup_scripts::{get_builtin_script, is_builtin_script};
pub use usacloud::{CreateServerConfig, NoteInfo, ServerInfo, SshKeyInfo, Usacloud};
