use std::sync::mpsc::Receiver;

use anyhow::Error;
use dataflow_types::{ExternalSourceConnector, PrometheusRegistry, PrometheusSourceConnector};
use expr::SourceInstanceId;
use prometheus::Registry;

use super::{SourceConstructor, SourceInfo};

type Out = Vec<u8>;
struct InternalMessage(Out);

/// Information required to load data from a Prometheus registry.
pub struct PrometheusSourceInfo {
    /// The prometheus registry
    registry: Registry,
    //id: SourceInstanceId,
    //receiver_stream: Receiver<Result<InternalMessage, Error>>,
}

impl SourceConstructor<Vec<u8>> for PrometheusSourceInfo {
    fn new(
        source_name: String,
        source_id: SourceInstanceId,
        active: bool,
        worker_id: usize,
        worker_count: usize,
        logger: Option<crate::logging::materialized::Logger>,
        consumer_activator: timely::scheduling::SyncActivator,
        connector: dataflow_types::ExternalSourceConnector,
        consistency_info: &mut super::ConsistencyInfo,
        encoding: dataflow_types::DataEncoding,
    ) -> Result<Self, Error>
    where
        Self: Sized + SourceInfo<Vec<u8>>,
    {
        match connector {
            ExternalSourceConnector::Prometheus(PrometheusSourceConnector {
                registry: PrometheusRegistry::Global,
            }) => (),
            _ => {
                panic!("Prometheus can only import from the global registry for now.");
            }
        };
        Ok(PrometheusSourceInfo {
            registry: prometheus::default_registry().clone(),
        })
    }
}

impl SourceInfo<Vec<u8>> for PrometheusSourceInfo {
    fn ensure_has_partition(
        &mut self,
        consistency_info: &mut super::ConsistencyInfo,
        pid: expr::PartitionId,
    ) {
        todo!()
    }

    fn update_partition_count(
        &mut self,
        consistency_info: &mut super::ConsistencyInfo,
        partition_count: i32,
    ) {
        todo!()
    }

    fn get_next_message(&mut self) -> Result<super::NextMessage<Vec<u8>>, Error> {
        todo!()
    }
}
