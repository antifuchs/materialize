use prometheus::Registry;

pub struct Batch {
    name: String,
    // TODO: tags, kind, read timestamp.
    value: f64,
}

/// Scrapes the prometheus registry in a regular interval and sends back a [`Batch`] of metric data
/// that can be inserted into a table.
pub struct Scraper {
    tx: mpsc::UnboundedSender<Batch>,
    interval: Duration,
    registry: Registry,
}

impl Scraper {
    pub fn new(tx: mpsc::UnboundedSender<Batch>, interval: Duration) -> Self {
        Scraper { tx, interval }
    }
}
