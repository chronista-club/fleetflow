//! FleetFlow Docker Image Build functionality
//!
//! This crate provides Docker image build capabilities for FleetFlow,
//! including Dockerfile resolution, build context creation, image building,
//! and image pushing to container registries.

pub mod auth;
pub mod builder;
pub mod context;
pub mod error;
pub mod progress;
pub mod pusher;
pub mod resolver;

pub use auth::RegistryAuth;
pub use builder::ImageBuilder;
pub use context::ContextBuilder;
pub use error::{BuildError, BuildResult};
pub use progress::BuildProgress;
pub use pusher::{ImagePusher, resolve_tag, split_image_tag};
pub use resolver::BuildResolver;
