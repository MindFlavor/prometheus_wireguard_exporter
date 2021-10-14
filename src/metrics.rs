use prometheus_exporter_base::{
    MetricType, MissingValue, PrometheusInstance, PrometheusMetric, Yes,
};

#[derive(Debug, Clone)]
pub(crate) struct MetricAttributeOptions {
    pub split_allowed_ips: bool,
    pub export_remote_ip_and_port: bool,
    pub handshake_timeout_seconds: Option<u64>,
}

pub struct EndpointMetrics<'a> {
    pub pc_sent_bytes_total: PrometheusMetric<'a>,
    pub pc_received_bytes_total: PrometheusMetric<'a>,
    pub pc_latest_handshake: PrometheusMetric<'a>,
}

impl<'a> EndpointMetrics<'a> {
    pub fn new() -> EndpointMetrics<'a> {
        return EndpointMetrics {
            pc_sent_bytes_total: PrometheusMetric::build()
                .with_name("wireguard_sent_bytes_total")
                .with_metric_type(MetricType::Counter)
                .with_help("Bytes sent to the peer")
                .build(),
            pc_received_bytes_total: PrometheusMetric::build()
                .with_name("wireguard_received_bytes_total")
                .with_metric_type(MetricType::Counter)
                .with_help("Bytes received from the peer")
                .build(),
            pc_latest_handshake: PrometheusMetric::build()
                .with_name("wireguard_latest_handshake_seconds")
                .with_metric_type(MetricType::Gauge)
                .with_help("Seconds from the last handshake")
                .build(),
        };
    }

    pub fn sent_bytes_total(
        &mut self,
        instance: &PrometheusInstance<u128, MissingValue>,
        bytes: u128,
    ) {
        self.pc_sent_bytes_total
            .render_and_append_instance(&instance.clone().with_value(bytes))
            .render();
    }

    pub fn received_bytes_total(
        &mut self,
        instance: &PrometheusInstance<u128, MissingValue>,
        bytes: u128,
    ) {
        self.pc_received_bytes_total
            .render_and_append_instance(&instance.clone().with_value(bytes))
            .render();
    }

    pub fn latest_handshake(
        &mut self,
        instance: &PrometheusInstance<u128, MissingValue>,
        latest: u128,
    ) {
        self.pc_latest_handshake
            .render_and_append_instance(&instance.clone().with_value(latest))
            .render();
    }
}

pub struct InterfaceMetrics<'a> {
    pub total_peers_gauge: PrometheusMetric<'a>,
}

impl<'a> InterfaceMetrics<'a> {
    pub fn new() -> InterfaceMetrics<'a> {
        return InterfaceMetrics {
            total_peers_gauge: PrometheusMetric::build()
                .with_name("wireguard_peers_total")
                .with_metric_type(MetricType::Gauge)
                .with_help("Total number of peers")
                .build(),
        };
    }

    pub fn connected_peers(&mut self, instance: &PrometheusInstance<usize, Yes>) {
        self.total_peers_gauge
            .render_and_append_instance(instance)
            .render();
    }
}
