use async_trait::async_trait;
use dataflow_types::{PrometheusRegistry, PrometheusSourceConnector, SourceError};
use prometheus::proto::MetricType;
use repr::{Datum, RowPacker};

use super::{SimpleSource, Timestamper};

/// Information required to load data from a Prometheus registry.
pub struct PrometheusSourceReader {
    /// The prometheus registry
    connector: PrometheusSourceConnector,
}

impl PrometheusSourceReader {
    /// Constructs a new prometheus source that gathers metrics from a registry.
    pub fn new(connector: PrometheusSourceConnector) -> Self {
        Self { connector }
    }
}

#[async_trait]
impl SimpleSource for PrometheusSourceReader {
    async fn start(mut self, timestamper: &Timestamper) -> Result<(), SourceError> {
        let mut packer = RowPacker::new();
        let registry = match self.connector.registry {
            PrometheusRegistry::Global => prometheus::default_registry(),
        };
        loop {
            tokio::time::sleep(self.connector.poll_interval).await;
            let tx = timestamper.start_tx().await;
            let reading = registry.gather();
            for family in reading.into_iter() {
                let name = family.get_name();
                for metric in family.get_metric().into_iter() {
                    let labels = metric
                        .get_label()
                        .into_iter()
                        .map(|pair| (pair.get_name(), Datum::from(pair.get_value())));
                    match family.get_field_type() {
                        MetricType::COUNTER => {
                            packer.push(Datum::from("counter"));
                            packer.push(Datum::from(name));
                            packer.push_dict(labels);
                            tx.insert(packer.finish_and_reuse())
                                .await
                                .map_err(|e| SourceError::FileIO(e.to_string()))?;
                        }
                        _ => {
                            // todo!("need more metric types")
                        }
                    }
                }
            }
        }
    }
}
