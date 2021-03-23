use std::time::Duration;

use prometheus::Registry;
use tokio::sync::mpsc;

pub struct Batch {
    name: String,
    // TODO: tags, kind, read timestamp.
    value: f64,
}

/// Scrapes the prometheus registry in a regular interval and sends back a [`Batch`] of metric data
/// that can be inserted into a table.
pub struct Scraper<'a> {
    tx: mpsc::UnboundedSender<Batch>,
    interval: Duration,
    registry: &'a Registry,
}

impl<'a> Scraper<'a> {
    pub fn new(
        tx: mpsc::UnboundedSender<Batch>,
        interval: Duration,
        registry: &'a Registry,
    ) -> Self {
        Scraper {
            tx,
            interval,
            registry,
        }
    }
}
