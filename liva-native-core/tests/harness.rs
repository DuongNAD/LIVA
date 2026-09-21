//! Unified Integration Test Harness for LIVA Native Core
//!
//! Consolidates all integration test suites into a single test target,
//! dramatically reducing linking overhead and compilation footprint.

#![allow(unused_imports, unused_variables, unused_mut)]

pub mod common;

#[path = "suites/storage.rs"]
pub mod storage;

#[path = "suites/llm.rs"]
pub mod llm;

#[path = "suites/voice.rs"]
pub mod voice;

#[path = "suites/vision.rs"]
pub mod vision;

#[path = "suites/agent.rs"]
pub mod agent;

#[path = "suites/security.rs"]
pub mod security;
