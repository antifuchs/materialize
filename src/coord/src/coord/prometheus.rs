use std::time::Duration;

use prometheus::{
    proto::{Metric, MetricFamily, MetricType},
    Registry,
};
use tokio::time;

use super::Coordinator;

/// Scrapes the prometheus registry in a regular interval and sends back a [`Batch`] of metric data
/// that can be inserted into a table.
pub struct Scraper<'a> {
    interval: Duration,
    registry: &'a Registry,
    // TODO: global ID for insertions
}

struct Reading {
    name: String,
    kind: MetricType,
    labels: Vec<(String, String)>,
    value: f64,
}

fn convert_metrics_to_rows<M: IntoIterator<Item = Metric>>(
    name: String,
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
                    name,
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

impl<'a> Scraper<'a> {
    pub fn new(interval: Duration, registry: &'a Registry) -> Self {
        Scraper { interval, registry }
    }

    /// Run forever: Scrape the metrics registry once per interval, inserting metrics.
    pub async fn run(&mut self, coord: &Coordinator) {
        loop {
            // TODO: come up with an exit condition?

            let metric_fams = self.registry.gather();
            let rows = metric_fams.into_iter().flat_map(
                |MetricFamily {
                     name,
                     help,
                     field_type,
                     metric,
                 }| { convert_metrics_to_rows(name, field_type, metric) },
            );

            time::sleep(self.interval).await;
        }
    }
}
