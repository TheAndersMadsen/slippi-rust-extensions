//! This module implements native Discord integration for Slippi.
//!
//! The core of it runs in a background thread, listening for new
//! events on each pass of its own loop.

use std::sync::mpsc::{channel, Receiver, Sender};
use std::thread;

use dolphin_integrations::Log;

mod config;
pub use config::Config;

mod error;
pub use error::DiscordRPCError;

pub(crate) type Result<T> = std::result::Result<T, DiscordRPCError>;

/// Message payloads that the inner thread listens for.
#[derive(Debug)]
pub enum Message {
    Dropping,
    UpdateConfig(Config),
}

/// A client that watches for game events and emits status updates to
/// Discord. This is effectively just a message passing route for the
/// background thread, which does all the actual work.
#[derive(Debug)]
pub struct DiscordHandler {
    tx: Sender<Message>,
}

impl DiscordHandler {
    /// Kicks off the background thread, which monitors game state and emits
    /// updates to Discord accordingly.
    pub fn new(ram_offset: usize, config: Config) -> Result<Self> {
        tracing::info!(target: Log::DiscordRPC, "Initializing DiscordHandler");

        // Create a sender and receiver channel pair to communicate between threads.
        let (tx, rx) = channel::<Message>();

        // Spawn a new background thread that manages its own loop. If or when
        // the loop breaks - either due to shutdown or intentional drop - the underlying
        // OS thread will clean itself up.
        thread::Builder::new()
            .name("DiscordHandler".to_string())
            .spawn(move || {
                if let Err(e) = Self::start(rx, ram_offset, config) {
                    tracing::error!(
                        target: Log::DiscordRPC,
                        error = ?e,
                        "DiscordHandler thread encountered an error: {e}"
                    );
                }
            })
            .map_err(error::DiscordRPCError::ThreadSpawn)?;

        Ok(Self { tx })
    }

    /// Must be called on a background thread. Runs the core event loop.
    fn start(rx: Receiver<Message>, ram_offset: usize, config: Config) -> Result<()> {
        loop {
            match rx.recv()? {
                // Handle any configuration updates.
                Message::UpdateConfig(config) => {},

                // Just break the loop so things exit cleanly.
                Message::Dropping => {
                    break;
                },
            }
        }

        Ok(())
    }

    /// Passes a new configuration into the background handler.
    pub fn update_config(&mut self, config: Config) {
        if let Err(e) = self.tx.send(Message::UpdateConfig(config)) {
            // @TODO: Maybe add an OSD log message here?

            tracing::error!(
                target: Log::DiscordRPC,
                error = ?e,
                "Failed to update DiscordHandler config"
            );
        }
    }
}

impl Drop for DiscordHandler {
    /// Notifies the background thread that we're dropping. The thread should
    /// listen for the message and break its runloop accordingly.
    fn drop(&mut self) {
        tracing::info!(target: Log::DiscordRPC, "Dropping DiscordHandler");

        if let Err(e) = self.tx.send(Message::Dropping) {
            tracing::warn!(
                target: Log::DiscordRPC,
                error = ?e,
                "Failed to notify child thread that DiscordHandler is dropping"
            );
        }
    }
}
