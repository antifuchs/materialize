// Copyright Materialize, Inc. All rights reserved.
//
// Use of this software is governed by the Business Source License
// included in the LICENSE file.
//
// As of the Change Date specified in that file, in accordance with
// the Business Source License, use of this software will be governed
// by the Apache License, Version 2.0.

//! A task that scrapes materialized's prometheus metrics and sends them to our logging tables.

use std::{
    convert::TryInto,
    iter::FromIterator,
    thread,
    time::{Duration, UNIX_EPOCH},
};

use chrono::NaiveDateTime;
use dataflow::{
    logging::materialized::{MaterializedEvent, Metric, MetricReading, MetricValue},
    SequencedCommand,
};
use prometheus::{proto::MetricType, Registry};
use repr::{Datum, Row, Timestamp};
use tokio::sync::mpsc::UnboundedSender;

use crate::catalog::builtin::BuiltinTable;

use super::catalog::builtin::{MZ_PROMETHEUS_HISTOGRAMS, MZ_PROMETHEUS_READINGS};
use super::{CatalogViewUpdate, Message};

/// Scrapes the prometheus registry in a regular interval and submits a batch of metric data to a
/// logging worker, to be inserted into a table.
pub struct Scraper<'a> {
    interval: Duration,
    retain_for: u64,
    registry: &'a Registry,
    command_rx: std::sync::mpsc::Receiver<ScraperMessage>,
    internal_tx: UnboundedSender<super::Message>,
}

#[derive(Clone, PartialEq, Debug)]
pub enum ScraperMessage {
    Shutdown,
}

/// The kind of a prometheus metric in a batch of metrics
#[derive(Debug, Clone, PartialOrd, PartialEq, Eq, Ord, Hash)]
enum MetricKind {
    /// A prometheus counter
    Counter,
    /// A prometheus gauge
    Gauge,
    /// A prometheus summary
    Summary,
    /// A prometheus histogram
    Histogram,
    /// An untyped metric
    Untyped,
}

impl MetricKind {
    fn as_str(&self) -> &'static str {
        use MetricKind::*;
        match self {
            Counter => "counter",
            Gauge => "gauge",
            Summary => "summary",
            Histogram => "histogram",
            Untyped => "untyped",
        }
    }
}

impl From<prometheus::proto::MetricType> for MetricKind {
    fn from(f: prometheus::proto::MetricType) -> Self {
        use prometheus::proto::MetricType::*;
        match f {
            COUNTER => MetricKind::Counter,
            GAUGE => MetricKind::Gauge,
            SUMMARY => MetricKind::Summary,
            HISTOGRAM => MetricKind::Histogram,

            UNTYPED => MetricKind::Untyped,
        }
    }
}

/// Information about the prometheus metric.
#[derive(Debug, Clone, PartialOrd, PartialEq, Eq, Ord, Hash)]
struct MetricMeta {
    name: String,
    kind: MetricKind,
    help: String,
}

impl MetricMeta {
    fn as_packed_row(&self) -> repr::Row {
        Row::pack_slice(&[
            Datum::from(self.name.as_str()),
            Datum::from(self.kind.as_str()),
            Datum::from(self.help.as_str()),
        ])
    }
}

fn convert_metrics_to_value_rows<'a, M: IntoIterator<Item = &'a prometheus::proto::Metric>>(
    name: &'a str,
    timestamp: NaiveDateTime,
    kind: MetricType,
    metrics: M,
) -> Vec<Row> {
    let mut row_packer = Row::default();
    metrics
        .into_iter()
        .flat_map(|m| {
            use MetricType::*;
            let labels: Vec<_> = m
                .get_label()
                .into_iter()
                .map(|pair| (pair.get_name(), Datum::from(pair.get_value())))
                .collect();
            match kind {
                COUNTER => {
                    row_packer.push(Datum::from(name));
                    row_packer.push(Datum::from(timestamp));
                    row_packer.push_dict(labels);
                    row_packer.push(Datum::from(m.get_counter().get_value()));
                    Some(row_packer.finish_and_reuse())
                }
                GAUGE => {
                    row_packer.push(Datum::from(name));
                    row_packer.push(Datum::from(timestamp));
                    row_packer.push_dict(labels);
                    row_packer.push(Datum::from(m.get_gauge().get_value()));
                    Some(row_packer.finish_and_reuse())
                }
                _ => None,
            }
        })
        .collect()
}

fn convert_metrics_to_histogram_rows<'a, M: IntoIterator<Item = &'a prometheus::proto::Metric>>(
    name: &'a str,
    timestamp: NaiveDateTime,
    kind: MetricType,
    metrics: M,
) -> Vec<Row> {
    let mut row_packer = Row::default();
    let mut rows: Vec<Row> = vec![];
    for metric in metrics {
        let labels: Vec<_> = metric
            .get_label()
            .into_iter()
            .map(|pair| (pair.get_name(), Datum::from(pair.get_value())))
            .collect();
        if kind == MetricType::HISTOGRAM {
            for bucket in metric.get_histogram().get_bucket() {
                row_packer.push(Datum::from(name));
                row_packer.push(Datum::from(timestamp));
                row_packer.push_dict(labels.iter().copied());
                row_packer.push(Datum::from(bucket.get_upper_bound()));
                row_packer.push(Datum::from(bucket.get_cumulative_count() as i64));
                rows.push(row_packer.finish_and_reuse());
            }
        }
    }
    rows
}

impl<'a> Scraper<'a> {
    pub fn new(
        interval: Duration,
        retain_for: Duration,
        registry: &'a Registry,
        command_rx: std::sync::mpsc::Receiver<ScraperMessage>,
        internal_tx: UnboundedSender<super::Message>,
    ) -> Self {
        let retain_for = retain_for.as_millis() as u64;
        Scraper {
            interval,
            retain_for,
            registry,
            command_rx,
            internal_tx,
        }
    }

    /// Run forever: Scrape the metrics registry once per interval, telling the coordinator to
    /// insert the values and meta-info in internal tables.
    pub fn run(&mut self) {
        loop {
            thread::sleep(self.interval);
            let timestamp: Timestamp = UNIX_EPOCH
                .elapsed()
                .expect("system clock is recent enough")
                .as_millis()
                .try_into()
                .expect("materialized is younger than 550M years.");
            if let Ok(cmd) = self.command_rx.try_recv() {
                match cmd {
                    ScraperMessage::Shutdown => return,
                }
            }

            let timestamp = NaiveDateTime::from_timestamp(0, 0)
                + chrono::Duration::from_std(Duration::from_millis(timestamp))
                    .expect("Couldn't convert timestamps");
            let metric_fams = self.registry.gather();

            let value_readings: Vec<Row> = metric_fams
                .iter()
                .flat_map(|family| {
                    convert_metrics_to_value_rows(
                        family.get_name(),
                        timestamp,
                        family.get_field_type().into(),
                        family.get_metric().into_iter(),
                    )
                })
                .collect();
            self.send_expiring_update(&MZ_PROMETHEUS_READINGS, value_readings);

            let histo_readings: Vec<Row> = metric_fams
                .iter()
                .flat_map(|family| {
                    convert_metrics_to_histogram_rows(
                        family.get_name(),
                        timestamp,
                        family.get_field_type().into(),
                        family.get_metric().into_iter(),
                    )
                })
                .collect();
            self.send_expiring_update(&MZ_PROMETHEUS_HISTOGRAMS, histo_readings);
        }
    }

    fn send_expiring_update(&self, table: &BuiltinTable, updates: Vec<Row>) {
        self.internal_tx
            .send(Message::InsertCatalogUpdates(CatalogViewUpdate {
                index_id: table.id,
                timestamp_offset: 0,
                updates: updates.iter().cloned().map(|metric| (metric, 1)).collect(),
            }))
            .expect("Sending positive metric reading messages");
        self.internal_tx
            .send(Message::InsertCatalogUpdates(CatalogViewUpdate {
                index_id: table.id,
                timestamp_offset: self.retain_for,
                updates: updates.iter().cloned().map(|metric| (metric, -1)).collect(),
            }))
            .expect("Sending metric reading retraction messages");
    }
}
