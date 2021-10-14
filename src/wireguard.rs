use crate::exporter_error::ExporterError;
use crate::metrics::{EndpointMetrics, InterfaceMetrics, MetricAttributeOptions};
use crate::wireguard_config::PeerEntryHashMap;
use crate::FriendlyDescription;
use log::{debug, trace};
use prometheus_exporter_base::PrometheusInstance;
use regex::Regex;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::fmt::Debug;
use std::net::SocketAddr;
use std::time::{SystemTime, UNIX_EPOCH};

const EMPTY: &str = "(none)";

#[derive(Default, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct SecureString(String);

#[cfg(feature = "leaky_log")]
impl Debug for SecureString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[cfg(not(feature = "leaky_log"))]
impl Debug for SecureString {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("**hidden**")
    }
}

impl From<&str> for SecureString {
    fn from(s: &str) -> Self {
        Self(s.to_owned())
    }
}

#[derive(Default, Debug, Clone)]
pub(crate) struct LocalEndpoint {
    pub public_key: String,
    pub private_key: SecureString,
    pub local_port: u16,
    pub persistent_keepalive: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct RemoteEndpoint {
    pub public_key: String,
    pub remote_ip: Option<String>,
    pub remote_port: Option<u16>,
    pub allowed_ips: String,
    pub latest_handshake: u64,
    pub sent_bytes: u128,
    pub received_bytes: u128,
    pub persistent_keepalive: bool,
}

#[derive(Debug, Clone)]
pub(crate) enum Endpoint {
    Local(LocalEndpoint),
    Remote(RemoteEndpoint),
}

fn to_option_string(s: &str) -> Option<String> {
    if s == EMPTY {
        None
    } else {
        Some(s.to_owned())
    }
}

fn to_bool(s: &str) -> bool {
    s != "off"
}

#[derive(Debug, Clone)]
pub(crate) struct WireGuard {
    pub interfaces: HashMap<String, Vec<Endpoint>>,
}

impl TryFrom<&str> for WireGuard {
    type Error = ExporterError;

    fn try_from(input: &str) -> Result<Self, Self::Error> {
        debug!("WireGuard::try_from({}) called", input);
        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        for line in input.lines() {
            let v: Vec<&str> = line.split('\t').filter(|s| !s.is_empty()).collect();
            debug!("WireGuard::try_from v == {:?}", v);

            let endpoint = if v.len() == 5 {
                // this is the local interface
                Endpoint::Local(LocalEndpoint {
                    public_key: v[1].to_owned(),
                    private_key: v[2].into(),
                    local_port: v[3].parse::<u16>().unwrap(),
                    persistent_keepalive: to_bool(v[4]),
                })
            } else {
                // remote endpoint
                let public_key = v[1].to_owned();

                let (remote_ip, remote_port) = if let Some(ip_and_port) = to_option_string(v[3]) {
                    // this workaround fixes issue #10 (see
                    // https://github.com/MindFlavor/prometheus_wireguard_exporter/issues/10).
                    // Whenever it will be fixed upstream this code will be replaced with a
                    // simple
                    // let addr: SocketAddr = ip_and_port.parse::<SocketAddr>().unwrap();
                    let re =
                        Regex::new(r"^\[(?P<ip>[A-Fa-f0-9:]+)%(.*)\]:(?P<port>[0-9]+)$").unwrap();
                    let addr: SocketAddr = re
                        .replace_all(&ip_and_port, "[$ip]:$port")
                        .parse::<SocketAddr>()
                        .unwrap();

                    (Some(addr.ip().to_string()), Some(addr.port()))
                } else {
                    (None, None)
                };

                let allowed_ips = v[4].to_owned();

                Endpoint::Remote(RemoteEndpoint {
                    public_key,
                    remote_ip,
                    remote_port,
                    allowed_ips,
                    latest_handshake: v[5].parse::<u64>()?,
                    received_bytes: v[6].parse::<u128>().unwrap(),
                    sent_bytes: v[7].parse::<u128>().unwrap(),
                    persistent_keepalive: to_bool(v[8]),
                })
            };

            trace!("WireGuard::try_from endpoint == {:?}", endpoint);

            if let Some(endpoints) = wg.interfaces.get_mut(v[0]) {
                endpoints.push(endpoint);
            } else {
                let new_vec = vec![endpoint];
                wg.interfaces.insert(v[0].to_owned(), new_vec);
            }
        }

        trace!("{:?}", wg);
        Ok(wg)
    }
}

impl WireGuard {
    pub fn merge(&mut self, merge_from: &WireGuard) {
        for (interface_name, endpoints_to_merge) in merge_from.interfaces.iter() {
            if let Some(endpoints) = self.interfaces.get_mut(interface_name) {
                endpoints.extend_from_slice(endpoints_to_merge);
            } else {
                let mut new_vec = Vec::new();
                new_vec.extend_from_slice(endpoints_to_merge);
                self.interfaces.insert(interface_name.to_owned(), new_vec);
            }
        }
    }

    pub(crate) fn render_with_names(
        &self,
        pehm: Option<&PeerEntryHashMap>,
        metric_attribute_options: &MetricAttributeOptions,
    ) -> String {
        debug!("WireGuard::render_with_names(self == {:?}, pehm == {:?}, split_allowed_ips == {:?}, export_remote_ip_and_port == {:?} called",
            self, pehm, metric_attribute_options.split_allowed_ips, metric_attribute_options.export_remote_ip_and_port);

        // these are the exported counters
        let mut endpoint_metrics = EndpointMetrics::new();
        let mut interface_metrics = InterfaceMetrics::new();

        // Here we make sure we process the interfaces in the
        // lexicographical order.
        // This is not stricly necessary but it ensures
        // a consistent output between executions (the iter() function
        // of HashMap does not guarantee any ordering).
        // Prometheus does not care about ordering but humans do so
        // we'll sort it beforehand. Being references the cost
        // should be negligible anyway.
        let mut interfaces_sorted: Vec<(&String, &Vec<Endpoint>)> = self
            .interfaces
            .iter()
            .collect::<Vec<(&String, &Vec<Endpoint>)>>();
        interfaces_sorted.sort_by(|a, b| a.0.partial_cmp(b.0).unwrap());

        for (interface, endpoints) in interfaces_sorted.into_iter() {
            let remote_endpoints: Vec<&RemoteEndpoint> = endpoints
                .iter()
                .map(|endpoint| {
                    self.populate_remote_endpoint_metrics(
                        pehm,
                        interface,
                        endpoint,
                        &mut endpoint_metrics,
                        metric_attribute_options,
                    )
                })
                .flatten()
                .collect();

            self.populate_interface_metrics(
                interface,
                &remote_endpoints,
                &mut interface_metrics,
                metric_attribute_options,
            );
        }

        format!(
            "{}\n{}\n{}\n{}",
            endpoint_metrics.pc_sent_bytes_total.render(),
            endpoint_metrics.pc_received_bytes_total.render(),
            endpoint_metrics.pc_latest_handshake.render(),
            interface_metrics.total_peers_gauge.render()
        )
    }

    pub(self) fn populate_interface_metrics(
        &self,
        interface: &str,
        remote_endpoints: &[&RemoteEndpoint],
        interface_metrics: &mut InterfaceMetrics,
        metric_attribute_options: &MetricAttributeOptions,
    ) {
        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let instance = PrometheusInstance::new().with_label("interface", interface);
        if let Some(handshake_timeout_seconds) = metric_attribute_options.handshake_timeout_seconds
        {
            let connected_endpoints: Vec<&&RemoteEndpoint> = remote_endpoints
                .iter()
                .filter(|&endpoint| {
                    since_the_epoch - endpoint.latest_handshake < handshake_timeout_seconds
                })
                .collect();

            let seen_recently = instance
                .clone()
                .with_label("seen_recently", "true")
                .with_value(connected_endpoints.len());
            interface_metrics.connected_peers(&seen_recently);

            let not_seen_recently = instance
                .clone()
                .with_label("seen_recently", "false")
                .with_value(remote_endpoints.len() - connected_endpoints.len());

            interface_metrics.connected_peers(&not_seen_recently);
        } else {
            let set = instance.with_value(remote_endpoints.len());
            interface_metrics.connected_peers(&set);
        }
    }

    pub(self) fn populate_remote_endpoint_metrics<'a>(
        &self,
        pehm: Option<&PeerEntryHashMap>,
        interface: &str,
        endpoint: &'a Endpoint,
        endpoint_metrics: &mut EndpointMetrics,
        metric_attribute_options: &MetricAttributeOptions,
    ) -> Option<&'a RemoteEndpoint> {
        // only show remote endpoints
        if let Endpoint::Remote(ep) = endpoint {
            debug!("WireGuard::render_with_names ep == {:?}", ep);

            // we store in attributes_owned the ownership of the values in order to
            // store in attributes their references. attributes_owned is only
            // needed for separate ip+subnet
            let mut attributes_owned: Vec<(String, String)> = Vec::new();
            let mut attributes: Vec<(&str, &str)> =
                vec![("interface", interface), ("public_key", &ep.public_key)];

            if metric_attribute_options.split_allowed_ips {
                struct NetworkingAddress<'a> {
                    ip: &'a str,
                    subnet: &'a str,
                }
                let networking_addresses: Vec<NetworkingAddress> = ep
                    .allowed_ips
                    .split(',')
                    .map(|ip_and_subnet| {
                        debug!(
                            "WireGuard::render_with_names ip_and_subnet == {:?}",
                            ip_and_subnet
                        );
                        let tokens: Vec<&str> = ip_and_subnet.split('/').collect();
                        debug!("WireGuard::render_with_names tokens == {:?}", tokens);

                        match (tokens.first(), tokens.last()) {
                            (Some(ip), Some(subnet)) => Some(NetworkingAddress { ip, subnet }),
                            _ => None,
                        }
                    })
                    .flatten()
                    .collect();

                for (idx, networking_address) in networking_addresses.iter().enumerate() {
                    attributes_owned.push((
                        format!("allowed_ip_{}", idx),
                        networking_address.ip.to_string(),
                    ));
                    attributes_owned.push((
                        format!("allowed_subnet_{}", idx),
                        networking_address.subnet.to_string(),
                    ));
                }
                debug!(
                    "WireGuard::render_with_names attributes == {:?}",
                    attributes
                );
            } else {
                attributes.push(("allowed_ips", &ep.allowed_ips));
            }

            // let's add the friendly_name attribute if present
            // and has meaniningful value
            if let Some(pehm) = pehm {
                if let Some(ep_friendly_description) = pehm.get(&ep.public_key as &str) {
                    if let Some(friendly_description) =
                        &ep_friendly_description.friendly_description
                    {
                        match friendly_description {
                            FriendlyDescription::Name(name) => {
                                attributes.push(("friendly_name", name));
                            }
                            FriendlyDescription::Json(json) => {
                                // let's put them in a intermediate vector and then sort it
                                let mut v_temp = Vec::new();

                                json.iter().for_each(|(header, value)| {
                                    //attributes_owned
                                    v_temp.push((
                                        header.to_string(),
                                        match value {
                                            serde_json::Value::Number(number) => number.to_string(),
                                            serde_json::Value::String(s) => s.to_owned(),
                                            serde_json::Value::Bool(b) => b.to_string(),
                                            _ => {
                                                debug!("WireGuard::unsupported json value");
                                                "unsupported_json_value".to_owned()
                                            }
                                        },
                                    ));
                                });

                                v_temp.sort_by(|(k0, _), (k1, _)| k0.cmp(k1));

                                v_temp
                                    .into_iter()
                                    .for_each(|item| attributes_owned.push(item));
                            }
                        }
                    }
                }
            }

            if metric_attribute_options.export_remote_ip_and_port {
                if let Some(r_ip) = &ep.remote_ip {
                    attributes.push(("remote_ip", r_ip));
                }
                if let Some(r_port) = &ep.remote_port {
                    attributes_owned.push(("remote_port".to_string(), r_port.to_string()));
                }
            }

            for (label, val) in &attributes_owned {
                attributes.push((label, val));
            }

            let mut instance = PrometheusInstance::new();
            for (h, v) in attributes {
                instance = instance.with_label(h, v);
            }

            endpoint_metrics.sent_bytes_total(&instance, ep.sent_bytes);
            endpoint_metrics.received_bytes_total(&instance, ep.received_bytes);
            endpoint_metrics.latest_handshake(&instance, ep.latest_handshake.into());

            Some(ep)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT : &'static str = "wg0\t000q4qAC0ExW/BuGSmVR1nxH9JAXT6g9Wd3oEGy5lA=\t0000u8LWR682knVm350lnuqlCJzw5SNLW9Nf96P+m8=\t51820\toff
wg0\t2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk=\t(none)\t37.159.76.245:29159\t10.70.0.2/32,10.70.0.66/32\t1555771458\t10288508\t139524160\toff
wg0\tqnoxQoQI8KKMupLnSSureORV0wMmH7JryZNsmGVISzU=\t(none)\t(none)\t10.70.0.3/32\t0\t0\t0\toff
wg0\tL2UoJZN7RmEKsMmqaJgKG0m1S2Zs2wd2ptAf+kb3008=\t(none)\t(none)\t10.70.0.4/32\t0\t0\t0\toff
wg0\tMdVOIPKt9K2MPj/sO2NlWQbOnFJ6L/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.0.50/32\t0\t0\t0\toff
wg2\tMdVOIPKt9K2MPj/sO2NlWQbOnFJcL/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.5.50/32\t0\t0\t0\toff
pollo\tYdVOIPKt9K2MPsO2NlWQbOnFJcL/qX80mmhQwsUlA=\t(none)\t(none)\t10.70.70.50/32\t0\t0\t0\toff
wg0\t928vO9Lf4+Mo84cWu4k1oRyzf0AR7FTGoPKHGoTMSHk=\t(none)\t5.90.62.106:21741\t10.70.0.80/32\t1555344925\t283012\t6604620\toff
";

    const TEXT_ISSUE_19 : &'static str = "wg0\twJyy0Xcqk76dNQI8bnzaQvrtle5Od+wft1RBK3fC8kc=\tVfjHGauX8OxotDMm2vi3JdwOUTDsFbxCnyInJ/wAXlk=\t51820\toff
wg0\t923V/iAdcz8BcqB0Xo6pDJzARGBJCQ6fWe+peixQyB4=\t(none)\t10.211.123.112:51820\t10.90.0.10/32,10.0.1.0/24\t0\t0\t0\toff
wg0\t9M1fhLa9sIlT39z+SI/0a5H3mNSHYmM+NGA6sirD2nU=\t(none)\t10.211.123.113:51820\t10.90.0.3/32,10.198.171.0/24\t0\t0\t0\toff
wg0\tgnRKXngxSppcYegsg38kEFn5Lmk4NcnRXLcZTtg2A2E=\t(none)\t10.211.123.114:51820\t10.90.0.11/32,10.189.143.0/24\t0\t0\t0\toff
wg0\tYW7NBDEPXuW9GQlFWFzpgrivMxzdR55M8VOTX+E0thw=\t(none)\t10.211.123.115:51820\t10.90.0.12/32,10.0.2.0/24\t0\t0\t0\toff
wg0\teVfg1BH1hcteASE16+TjShxAJNyFLQ9QIcnCaylD/AA=\t(none)\t10.211.123.116:51820\t10.90.0.13/32,10.0.3.0/24\t0\t0\t0\toff
wg0\tlh1h+tWPahB+PAWW62ExHVVrOp9IwdjYwaGnPIXgNwY=\t(none)\t10.211.123.117:51820\t10.90.0.9/32,10.0.4.0/24\t0\t0\t0\toff
wg0\tVQIrk1BiBfbOkkKGPiarEvhA4iPuszIL1lddvvFDvE0=\t(none)\t10.211.123.118:51820\t10.90.0.8/32,10.0.5.0/24\t0\t0\t0\toff
wg0\tSMp58OwCNnwlzu+OdpA8xiNJzOwbl2gdMaD9CSZCC24=\t(none)\t10.211.123.119:51820\t10.90.0.14/32,10.0.6.0/24\t0\t0\t0\toff
wg0\t+0+yMIHVCqyIf4by1gxAjqQ92iKv3bQ/JctNVUEpSlU=\t(none)\t10.211.123.120:51820\t10.90.0.7/32,10.0.7.0/24\t0\t0\t0\toff
wg0\t2StYqQY9tyVkGcO4ykKTiTu6AQp/yIYx8I4hwBLO1jA=\t(none)\t10.211.123.121:51820\t10.90.0.15/32,10.0.8.0/24\t0\t0\t0\toff
wg0\tqa0AMD2puDBBrs8NYQ+skIrIi/Q5NgQRZLEh5p80Mnc=\t(none)\t10.211.123.122:51820\t10.90.0.1/32,10.0.10.0/24\t0\t0\t0\toff
wg0\tYwObmKDK4lfr5F6FHqJhDy9nkUQwbuK8wh4ac2VNSEU=\t(none)\t10.211.123.123:51820\t10.90.0.2/32,10.0.11.0/24\t0\t0\t0\toff
wg0\tq07dm9n1UMLFbG6Dh+BNztCt7jVb9VtpVshQEf580kA=\t(none)\t10.211.123.124:51820\t10.90.0.6/32,10.0.13.0/24\t0\t0\t0\toff
wg0\tyZOoC2t6pBcXvoczuiJqrQ+8CYvJCzcq8aqyp+APaAE=\t(none)\t10.211.123.125:51820\t10.90.0.16/32,10.0.14.0/24\t1574770531\t1232856\t12306832\toff
wg0\tyjeBkrZqUThSSHySFzWCjxAH8cxtiWSI2I8JFD6t1UM=\t(none)\t10.211.123.126:51820\t10.90.0.5/32\t1574770705\t18576788764\t10642564136\toff
wg0\tHtOSi37ALMnSkeAFqeWYZqlBnZqAJERhb5o/i3ZPEFI=\t(none)\t10.211.123.127:51820\t10.90.0.17/32\t1574770783\t62592693520\t1439257868\toff
wg0\tsUsR6xufQQ8Tf0FuyY9tfEeYdhVMeFelr4ZMUrj+B0E=\t(none)\t10.211.123.128:51820\t10.90.0.18/32\t1574770693\t75066288152\t1624251784\toff";

    #[test]
    fn test_parse_issue_19() {
        println!("starting debug");
        let a = WireGuard::try_from(TEXT_ISSUE_19).unwrap();
        assert!(a.interfaces.len() == 1);
        assert!(a.interfaces["wg0"].len() == 18);

        let e1 = match &a.interfaces["wg0"][1] {
            Endpoint::Local(_) => panic!(),
            Endpoint::Remote(re) => re,
        };
        assert_eq!(
            e1.public_key,
            "923V/iAdcz8BcqB0Xo6pDJzARGBJCQ6fWe+peixQyB4="
        );

        assert_eq!(e1.remote_ip, Some("10.211.123.112".to_owned()));
        assert_eq!(e1.allowed_ips, "10.90.0.10/32,10.0.1.0/24".to_owned());

        let e17 = match &a.interfaces["wg0"][17] {
            Endpoint::Local(_) => panic!(),
            Endpoint::Remote(re) => re,
        };
        assert_eq!(
            e17.public_key,
            "sUsR6xufQQ8Tf0FuyY9tfEeYdhVMeFelr4ZMUrj+B0E="
        );

        assert_eq!(e17.remote_ip, Some("10.211.123.128".to_owned()));
        assert_eq!(e17.allowed_ips, "10.90.0.18/32".to_owned());
        assert_eq!(e17.latest_handshake, 1574770693);
        assert_eq!(e17.sent_bytes, 1624251784);
        assert_eq!(e17.received_bytes, 75066288152);

        let pe = PeerEntryHashMap::new();

        let metric_attribute_options = MetricAttributeOptions {
            split_allowed_ips: true,
            export_remote_ip_and_port: true,
            handshake_timeout_seconds: None,
        };
        let s = a.render_with_names(Some(&pe), &metric_attribute_options);
        println!("{}", s);

        let s_ok = "# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"923V/iAdcz8BcqB0Xo6pDJzARGBJCQ6fWe+peixQyB4=\",remote_ip=\"10.211.123.112\",allowed_ip_0=\"10.90.0.10\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.1.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"9M1fhLa9sIlT39z+SI/0a5H3mNSHYmM+NGA6sirD2nU=\",remote_ip=\"10.211.123.113\",allowed_ip_0=\"10.90.0.3\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.198.171.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"gnRKXngxSppcYegsg38kEFn5Lmk4NcnRXLcZTtg2A2E=\",remote_ip=\"10.211.123.114\",allowed_ip_0=\"10.90.0.11\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.189.143.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"YW7NBDEPXuW9GQlFWFzpgrivMxzdR55M8VOTX+E0thw=\",remote_ip=\"10.211.123.115\",allowed_ip_0=\"10.90.0.12\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.2.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"eVfg1BH1hcteASE16+TjShxAJNyFLQ9QIcnCaylD/AA=\",remote_ip=\"10.211.123.116\",allowed_ip_0=\"10.90.0.13\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.3.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"lh1h+tWPahB+PAWW62ExHVVrOp9IwdjYwaGnPIXgNwY=\",remote_ip=\"10.211.123.117\",allowed_ip_0=\"10.90.0.9\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.4.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"VQIrk1BiBfbOkkKGPiarEvhA4iPuszIL1lddvvFDvE0=\",remote_ip=\"10.211.123.118\",allowed_ip_0=\"10.90.0.8\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.5.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"SMp58OwCNnwlzu+OdpA8xiNJzOwbl2gdMaD9CSZCC24=\",remote_ip=\"10.211.123.119\",allowed_ip_0=\"10.90.0.14\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.6.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"+0+yMIHVCqyIf4by1gxAjqQ92iKv3bQ/JctNVUEpSlU=\",remote_ip=\"10.211.123.120\",allowed_ip_0=\"10.90.0.7\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.7.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"2StYqQY9tyVkGcO4ykKTiTu6AQp/yIYx8I4hwBLO1jA=\",remote_ip=\"10.211.123.121\",allowed_ip_0=\"10.90.0.15\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.8.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"qa0AMD2puDBBrs8NYQ+skIrIi/Q5NgQRZLEh5p80Mnc=\",remote_ip=\"10.211.123.122\",allowed_ip_0=\"10.90.0.1\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.10.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"YwObmKDK4lfr5F6FHqJhDy9nkUQwbuK8wh4ac2VNSEU=\",remote_ip=\"10.211.123.123\",allowed_ip_0=\"10.90.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.11.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"q07dm9n1UMLFbG6Dh+BNztCt7jVb9VtpVshQEf580kA=\",remote_ip=\"10.211.123.124\",allowed_ip_0=\"10.90.0.6\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.13.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"yZOoC2t6pBcXvoczuiJqrQ+8CYvJCzcq8aqyp+APaAE=\",remote_ip=\"10.211.123.125\",allowed_ip_0=\"10.90.0.16\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.14.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 12306832
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"yjeBkrZqUThSSHySFzWCjxAH8cxtiWSI2I8JFD6t1UM=\",remote_ip=\"10.211.123.126\",allowed_ip_0=\"10.90.0.5\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 10642564136
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"HtOSi37ALMnSkeAFqeWYZqlBnZqAJERhb5o/i3ZPEFI=\",remote_ip=\"10.211.123.127\",allowed_ip_0=\"10.90.0.17\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 1439257868
wireguard_sent_bytes_total{interface=\"wg0\",public_key=\"sUsR6xufQQ8Tf0FuyY9tfEeYdhVMeFelr4ZMUrj+B0E=\",remote_ip=\"10.211.123.128\",allowed_ip_0=\"10.90.0.18\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 1624251784

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"923V/iAdcz8BcqB0Xo6pDJzARGBJCQ6fWe+peixQyB4=\",remote_ip=\"10.211.123.112\",allowed_ip_0=\"10.90.0.10\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.1.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"9M1fhLa9sIlT39z+SI/0a5H3mNSHYmM+NGA6sirD2nU=\",remote_ip=\"10.211.123.113\",allowed_ip_0=\"10.90.0.3\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.198.171.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"gnRKXngxSppcYegsg38kEFn5Lmk4NcnRXLcZTtg2A2E=\",remote_ip=\"10.211.123.114\",allowed_ip_0=\"10.90.0.11\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.189.143.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"YW7NBDEPXuW9GQlFWFzpgrivMxzdR55M8VOTX+E0thw=\",remote_ip=\"10.211.123.115\",allowed_ip_0=\"10.90.0.12\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.2.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"eVfg1BH1hcteASE16+TjShxAJNyFLQ9QIcnCaylD/AA=\",remote_ip=\"10.211.123.116\",allowed_ip_0=\"10.90.0.13\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.3.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"lh1h+tWPahB+PAWW62ExHVVrOp9IwdjYwaGnPIXgNwY=\",remote_ip=\"10.211.123.117\",allowed_ip_0=\"10.90.0.9\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.4.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"VQIrk1BiBfbOkkKGPiarEvhA4iPuszIL1lddvvFDvE0=\",remote_ip=\"10.211.123.118\",allowed_ip_0=\"10.90.0.8\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.5.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"SMp58OwCNnwlzu+OdpA8xiNJzOwbl2gdMaD9CSZCC24=\",remote_ip=\"10.211.123.119\",allowed_ip_0=\"10.90.0.14\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.6.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"+0+yMIHVCqyIf4by1gxAjqQ92iKv3bQ/JctNVUEpSlU=\",remote_ip=\"10.211.123.120\",allowed_ip_0=\"10.90.0.7\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.7.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"2StYqQY9tyVkGcO4ykKTiTu6AQp/yIYx8I4hwBLO1jA=\",remote_ip=\"10.211.123.121\",allowed_ip_0=\"10.90.0.15\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.8.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"qa0AMD2puDBBrs8NYQ+skIrIi/Q5NgQRZLEh5p80Mnc=\",remote_ip=\"10.211.123.122\",allowed_ip_0=\"10.90.0.1\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.10.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"YwObmKDK4lfr5F6FHqJhDy9nkUQwbuK8wh4ac2VNSEU=\",remote_ip=\"10.211.123.123\",allowed_ip_0=\"10.90.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.11.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"q07dm9n1UMLFbG6Dh+BNztCt7jVb9VtpVshQEf580kA=\",remote_ip=\"10.211.123.124\",allowed_ip_0=\"10.90.0.6\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.13.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"yZOoC2t6pBcXvoczuiJqrQ+8CYvJCzcq8aqyp+APaAE=\",remote_ip=\"10.211.123.125\",allowed_ip_0=\"10.90.0.16\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.14.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 1232856
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"yjeBkrZqUThSSHySFzWCjxAH8cxtiWSI2I8JFD6t1UM=\",remote_ip=\"10.211.123.126\",allowed_ip_0=\"10.90.0.5\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 18576788764
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"HtOSi37ALMnSkeAFqeWYZqlBnZqAJERhb5o/i3ZPEFI=\",remote_ip=\"10.211.123.127\",allowed_ip_0=\"10.90.0.17\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 62592693520
wireguard_received_bytes_total{interface=\"wg0\",public_key=\"sUsR6xufQQ8Tf0FuyY9tfEeYdhVMeFelr4ZMUrj+B0E=\",remote_ip=\"10.211.123.128\",allowed_ip_0=\"10.90.0.18\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 75066288152

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"923V/iAdcz8BcqB0Xo6pDJzARGBJCQ6fWe+peixQyB4=\",remote_ip=\"10.211.123.112\",allowed_ip_0=\"10.90.0.10\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.1.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"9M1fhLa9sIlT39z+SI/0a5H3mNSHYmM+NGA6sirD2nU=\",remote_ip=\"10.211.123.113\",allowed_ip_0=\"10.90.0.3\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.198.171.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"gnRKXngxSppcYegsg38kEFn5Lmk4NcnRXLcZTtg2A2E=\",remote_ip=\"10.211.123.114\",allowed_ip_0=\"10.90.0.11\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.189.143.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"YW7NBDEPXuW9GQlFWFzpgrivMxzdR55M8VOTX+E0thw=\",remote_ip=\"10.211.123.115\",allowed_ip_0=\"10.90.0.12\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.2.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"eVfg1BH1hcteASE16+TjShxAJNyFLQ9QIcnCaylD/AA=\",remote_ip=\"10.211.123.116\",allowed_ip_0=\"10.90.0.13\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.3.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"lh1h+tWPahB+PAWW62ExHVVrOp9IwdjYwaGnPIXgNwY=\",remote_ip=\"10.211.123.117\",allowed_ip_0=\"10.90.0.9\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.4.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"VQIrk1BiBfbOkkKGPiarEvhA4iPuszIL1lddvvFDvE0=\",remote_ip=\"10.211.123.118\",allowed_ip_0=\"10.90.0.8\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.5.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"SMp58OwCNnwlzu+OdpA8xiNJzOwbl2gdMaD9CSZCC24=\",remote_ip=\"10.211.123.119\",allowed_ip_0=\"10.90.0.14\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.6.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"+0+yMIHVCqyIf4by1gxAjqQ92iKv3bQ/JctNVUEpSlU=\",remote_ip=\"10.211.123.120\",allowed_ip_0=\"10.90.0.7\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.7.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"2StYqQY9tyVkGcO4ykKTiTu6AQp/yIYx8I4hwBLO1jA=\",remote_ip=\"10.211.123.121\",allowed_ip_0=\"10.90.0.15\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.8.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"qa0AMD2puDBBrs8NYQ+skIrIi/Q5NgQRZLEh5p80Mnc=\",remote_ip=\"10.211.123.122\",allowed_ip_0=\"10.90.0.1\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.10.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"YwObmKDK4lfr5F6FHqJhDy9nkUQwbuK8wh4ac2VNSEU=\",remote_ip=\"10.211.123.123\",allowed_ip_0=\"10.90.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.11.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"q07dm9n1UMLFbG6Dh+BNztCt7jVb9VtpVshQEf580kA=\",remote_ip=\"10.211.123.124\",allowed_ip_0=\"10.90.0.6\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.13.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 0
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"yZOoC2t6pBcXvoczuiJqrQ+8CYvJCzcq8aqyp+APaAE=\",remote_ip=\"10.211.123.125\",allowed_ip_0=\"10.90.0.16\",allowed_subnet_0=\"32\",allowed_ip_1=\"10.0.14.0\",allowed_subnet_1=\"24\",remote_port=\"51820\"} 1574770531
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"yjeBkrZqUThSSHySFzWCjxAH8cxtiWSI2I8JFD6t1UM=\",remote_ip=\"10.211.123.126\",allowed_ip_0=\"10.90.0.5\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 1574770705
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"HtOSi37ALMnSkeAFqeWYZqlBnZqAJERhb5o/i3ZPEFI=\",remote_ip=\"10.211.123.127\",allowed_ip_0=\"10.90.0.17\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 1574770783
wireguard_latest_handshake_seconds{interface=\"wg0\",public_key=\"sUsR6xufQQ8Tf0FuyY9tfEeYdhVMeFelr4ZMUrj+B0E=\",remote_ip=\"10.211.123.128\",allowed_ip_0=\"10.90.0.18\",allowed_subnet_0=\"32\",remote_port=\"51820\"} 1574770693

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{interface=\"wg0\"} 17
";
        assert_eq!(s, s_ok);
    }

    #[test]
    fn test_parse() {
        let a = WireGuard::try_from(TEXT).unwrap();
        println!("{:?}", a);
        assert!(a.interfaces.len() == 3);
        assert!(a.interfaces["wg0"].len() == 6);

        let e1 = match &a.interfaces["wg0"][1] {
            Endpoint::Local(_) => panic!(),
            Endpoint::Remote(re) => re,
        };

        assert_eq!(
            e1.public_key,
            "2S7mA0vEMethCNQrJpJKE81/JmhgtB+tHHLYQhgM6kk="
        );

        assert_eq!(e1.remote_ip, Some("37.159.76.245".to_owned()));
        assert_eq!(e1.allowed_ips, "10.70.0.2/32,10.70.0.66/32".to_owned());
    }

    #[test]
    fn test_parse_and_serialize() {
        let a = WireGuard::try_from(TEXT).unwrap();
        let metric_attribute_options = MetricAttributeOptions {
            split_allowed_ips: false,
            export_remote_ip_and_port: true,
            handshake_timeout_seconds: None,
        };
        let s = a.render_with_names(None, &metric_attribute_options);
        println!("{}", s);
    }

    #[test]
    fn test_render_to_prometheus_simple() {
        const REF : &str= "# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"to_change\",remote_ip=\"remote_ip\",remote_port=\"100\"} 1000

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"to_change\",remote_ip=\"remote_ip\",remote_port=\"100\"} 5000

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"to_change\",remote_ip=\"remote_ip\",remote_port=\"100\"} 500

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{interface=\"Pippo\"} 1
";

        let re = Endpoint::Remote(RemoteEndpoint {
            public_key: "test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            allowed_ips: "to_change".to_owned(),
            latest_handshake: 500,
            sent_bytes: 1000,
            received_bytes: 5000,
            persistent_keepalive: false,
        });
        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        let mut v = Vec::new();
        v.push(re);
        wg.interfaces.insert("Pippo".to_owned(), v);

        let metric_attribute_options = MetricAttributeOptions {
            split_allowed_ips: false,
            export_remote_ip_and_port: true,
            handshake_timeout_seconds: None,
        };
        let prometheus = wg.render_with_names(None, &metric_attribute_options);

        assert_eq!(prometheus, REF);
    }

    use crate::wireguard_config::PeerEntry;

    #[test]
    fn test_render_to_prometheus_with_handshake_timeout() {
        use std::time::{SystemTime, UNIX_EPOCH};

        let start = SystemTime::now();
        let since_the_epoch = start
            .duration_since(UNIX_EPOCH)
            .expect("Time went backwards")
            .as_secs();

        let handshake_timeout = 30;

        let re1 = RemoteEndpoint {
            public_key: "test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            allowed_ips: "10.0.0.2/32,fd86:ea04:::4/128".to_owned(),
            latest_handshake: since_the_epoch - handshake_timeout - 1,
            sent_bytes: 1000,
            received_bytes: 5000,
            persistent_keepalive: false,
        };
        let re2 = RemoteEndpoint {
            public_key: "second_test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            allowed_ips: "10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16".to_owned(),
            latest_handshake: since_the_epoch,
            sent_bytes: 14,
            received_bytes: 1_000_000_000,
            persistent_keepalive: false,
        };

        let handshake_timeout_output = format!("# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"}} 1000
wireguard_sent_bytes_total{{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",remote_port=\"100\"}} 14

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"}} 5000
wireguard_received_bytes_total{{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",remote_port=\"100\"}} 1000000000

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"}} {}
wireguard_latest_handshake_seconds{{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",remote_port=\"100\"}} {}

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{{interface=\"Pippo\",seen_recently=\"true\"}} 1
wireguard_peers_total{{interface=\"Pippo\",seen_recently=\"false\"}} 1
", re1.latest_handshake, re2.latest_handshake);

        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        let mut v = Vec::new();
        v.push(Endpoint::Remote(re1));
        v.push(Endpoint::Remote(re2));
        wg.interfaces.insert("Pippo".to_owned(), v);

        let mut pehm = PeerEntryHashMap::new();
        let pe = PeerEntry {
            public_key: "second_test",
            allowed_ips: "ignored",
            friendly_description: Some(FriendlyDescription::Name(
                "this is my friendly name".into(),
            )),
        };
        pehm.insert(pe.public_key, pe.clone());
        {
            let metric_attribute_options = MetricAttributeOptions {
                split_allowed_ips: false,
                export_remote_ip_and_port: true,
                handshake_timeout_seconds: Some(handshake_timeout),
            };
            let prometheus = wg.render_with_names(Some(&pehm), &metric_attribute_options);
            assert_eq!(prometheus, handshake_timeout_output);
        }
    }

    #[test]
    fn test_render_to_prometheus_complex() {
        const REF :&'static str = "# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"} 1000
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",remote_port=\"100\"} 14

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"} 5000
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",remote_port=\"100\"} 1000000000

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"} 500
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",remote_port=\"100\"} 50

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{interface=\"Pippo\"} 2
";

        const REF_SPLIT :&'static str = "# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"test\",remote_ip=\"remote_ip\",allowed_ip_0=\"10.0.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",remote_port=\"100\"} 1000
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"second_test\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",allowed_ip_0=\"10.0.0.4\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",allowed_ip_2=\"192.168.0.0\",allowed_subnet_2=\"16\",remote_port=\"100\"} 14

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"test\",remote_ip=\"remote_ip\",allowed_ip_0=\"10.0.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",remote_port=\"100\"} 5000
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"second_test\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",allowed_ip_0=\"10.0.0.4\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",allowed_ip_2=\"192.168.0.0\",allowed_subnet_2=\"16\",remote_port=\"100\"} 1000000000

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"test\",remote_ip=\"remote_ip\",allowed_ip_0=\"10.0.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",remote_port=\"100\"} 500
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"second_test\",friendly_name=\"this is my friendly name\",remote_ip=\"remote_ip\",allowed_ip_0=\"10.0.0.4\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",allowed_ip_2=\"192.168.0.0\",allowed_subnet_2=\"16\",remote_port=\"100\"} 50

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{interface=\"Pippo\"} 2
";

        const REF_SPLIT_NO_REMOTE :&'static str = "# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ip_0=\"10.0.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\"} 1000
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"second_test\",friendly_name=\"this is my friendly name\",allowed_ip_0=\"10.0.0.4\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",allowed_ip_2=\"192.168.0.0\",allowed_subnet_2=\"16\"} 14

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ip_0=\"10.0.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\"} 5000
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"second_test\",friendly_name=\"this is my friendly name\",allowed_ip_0=\"10.0.0.4\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",allowed_ip_2=\"192.168.0.0\",allowed_subnet_2=\"16\"} 1000000000

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"test\",allowed_ip_0=\"10.0.0.2\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\"} 500
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"second_test\",friendly_name=\"this is my friendly name\",allowed_ip_0=\"10.0.0.4\",allowed_subnet_0=\"32\",allowed_ip_1=\"fd86:ea04:::4\",allowed_subnet_1=\"128\",allowed_ip_2=\"192.168.0.0\",allowed_subnet_2=\"16\"} 50

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{interface=\"Pippo\"} 2
";

        const REF_JSON :&'static str = "# HELP wireguard_sent_bytes_total Bytes sent to the peer
# TYPE wireguard_sent_bytes_total counter
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"} 1000
wireguard_sent_bytes_total{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",remote_ip=\"remote_ip\",auth_date=\"1614869789\",first_name=\"Coordinator\",id=\"482217555\",last_name=\"DrProxy.me\",username=\"DrProxyMeCoordinator\",remote_port=\"100\"} 14

# HELP wireguard_received_bytes_total Bytes received from the peer
# TYPE wireguard_received_bytes_total counter
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"} 5000
wireguard_received_bytes_total{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",remote_ip=\"remote_ip\",auth_date=\"1614869789\",first_name=\"Coordinator\",id=\"482217555\",last_name=\"DrProxy.me\",username=\"DrProxyMeCoordinator\",remote_port=\"100\"} 1000000000

# HELP wireguard_latest_handshake_seconds Seconds from the last handshake
# TYPE wireguard_latest_handshake_seconds gauge
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"test\",allowed_ips=\"10.0.0.2/32,fd86:ea04:::4/128\",remote_ip=\"remote_ip\",remote_port=\"100\"} 500
wireguard_latest_handshake_seconds{interface=\"Pippo\",public_key=\"second_test\",allowed_ips=\"10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16\",remote_ip=\"remote_ip\",auth_date=\"1614869789\",first_name=\"Coordinator\",id=\"482217555\",last_name=\"DrProxy.me\",username=\"DrProxyMeCoordinator\",remote_port=\"100\"} 50

# HELP wireguard_peers_total Total number of peers
# TYPE wireguard_peers_total gauge
wireguard_peers_total{interface=\"Pippo\"} 2
";

        let re1 = Endpoint::Remote(RemoteEndpoint {
            public_key: "test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            allowed_ips: "10.0.0.2/32,fd86:ea04:::4/128".to_owned(),
            latest_handshake: 500,
            sent_bytes: 1000,
            received_bytes: 5000,
            persistent_keepalive: false,
        });
        let re2 = Endpoint::Remote(RemoteEndpoint {
            public_key: "second_test".to_owned(),
            remote_ip: Some("remote_ip".to_owned()),
            remote_port: Some(100),
            allowed_ips: "10.0.0.4/32,fd86:ea04:::4/128,192.168.0.0/16".to_owned(),
            latest_handshake: 50,
            sent_bytes: 14,
            received_bytes: 1_000_000_000,
            persistent_keepalive: false,
        });

        let mut wg = WireGuard {
            interfaces: HashMap::new(),
        };

        let mut v = Vec::new();
        v.push(re1);
        v.push(re2);
        wg.interfaces.insert("Pippo".to_owned(), v);

        let mut pehm = PeerEntryHashMap::new();
        let pe = PeerEntry {
            public_key: "second_test",
            allowed_ips: "ignored",
            friendly_description: Some(FriendlyDescription::Name(
                "this is my friendly name".into(),
            )),
        };
        pehm.insert(pe.public_key, pe.clone());

        {
            let metric_attribute_options = MetricAttributeOptions {
                split_allowed_ips: false,
                export_remote_ip_and_port: true,
                handshake_timeout_seconds: None,
            };

            let prometheus = wg.render_with_names(Some(&pehm), &metric_attribute_options);
            assert_eq!(prometheus, REF);
        }

        {
            let metric_attribute_options = MetricAttributeOptions {
                split_allowed_ips: true,
                export_remote_ip_and_port: true,
                handshake_timeout_seconds: None,
            };
            let prometheus = wg.render_with_names(Some(&pehm), &metric_attribute_options);
            assert_eq!(prometheus, REF_SPLIT);
        }

        {
            let metric_attribute_options = MetricAttributeOptions {
                split_allowed_ips: true,
                export_remote_ip_and_port: false,
                handshake_timeout_seconds: None,
            };
            let prometheus = wg.render_with_names(Some(&pehm), &metric_attribute_options);
            assert_eq!(prometheus, REF_SPLIT_NO_REMOTE);
        }

        // second test
        let mut pehm = PeerEntryHashMap::new();
        let mut hm = HashMap::new();
        hm.insert(
            "username",
            serde_json::Value::String("DrProxyMeCoordinator".to_owned()),
        );
        hm.insert("id", serde_json::Value::Number(482217555.into()));
        hm.insert(
            "first_name",
            serde_json::Value::String("Coordinator".to_owned()),
        );
        hm.insert(
            "last_name",
            serde_json::Value::String("DrProxy.me".to_owned()),
        );
        hm.insert("auth_date", serde_json::Value::Number(1614869789.into()));

        let pe = PeerEntry {
            public_key: "second_test",
            allowed_ips: "ignored",
            friendly_description: Some(FriendlyDescription::Json(hm)),
        };
        pehm.insert(pe.public_key, pe.clone());

        let metric_attribute_options = MetricAttributeOptions {
            split_allowed_ips: false,
            export_remote_ip_and_port: true,
            handshake_timeout_seconds: None,
        };
        let prometheus = wg.render_with_names(Some(&pehm), &metric_attribute_options);
        assert_eq!(prometheus, REF_JSON);
    }
}
