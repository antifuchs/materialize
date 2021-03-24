use std::time::Duration;

use prometheus::{
    proto::{Metric, MetricType},
    Registry,
};
use tokio::time;

use super::Coordinator;

/// Scrapes the prometheus registry in a regular interval and sends back a [`Batch`] of metric data
/// that can be inserted into a table.
pub struct Scraper {
    interval: Duration,
    registry: Registry,
    // TODO: global ID for insertions
}

pub struct Reading {
    name: String,
    kind: MetricType,
    labels: Vec<(String, String)>,
    value: f64,
}

fn convert_metrics_to_rows<'a, M: IntoIterator<Item = &'a Metric>>(
    name: &str,
    kind: MetricType,
    metrics: M,
) -> Vec<Reading> {
    metrics
        .into_iter()
        .flat_map(|m| {
            use MetricType::*;
            let labels = m
                .get_label()
                .into_iter()
                .map(|pair| (pair.get_name().to_owned(), pair.get_value().to_owned()))
                .collect::<Vec<(String, String)>>();
            match kind {
                COUNTER => vec![Reading {
                    name: name.to_owned(),
                    kind,
                    labels,
                    value: m.get_counter().get_value(),
                }],
                // TODO: gauges, histograms, summaries and more
                _ => vec![],
            }
        })
        .collect()
}

impl Scraper {
    pub fn new(interval: Duration, registry: &Registry) -> Self {
        let registry = registry.clone();
        Scraper { interval, registry }
    }

    /// Scrape the metrics registry once per interval, returning Reading values.
    pub async fn scrape(&mut self) -> Vec<Reading> {
        self.registry
            .gather()
            .into_iter()
            .flat_map(|mf| {
                convert_metrics_to_rows(mf.get_name(), mf.get_field_type(), mf.get_metric())
            })
            .collect()
    }
}
